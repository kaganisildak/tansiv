#![feature(try_from)]
#![feature(uniform_paths)]
// Use chrono::Duration (re-exported from time::Duration) to allow negative values, which are not
// allowed in std::time::Duration
use chrono::{Duration, naive::NaiveDateTime};
pub(crate) use config::Config;
use connector::{Connector, ConnectorImpl, MsgIn, MsgOut};
use libc;
use log::{debug, error};
use std::io::Result;
use std::os::raw::{c_char, c_int};

const MAX_PACKET_SIZE: usize = 2048;

mod config;
mod connector;

pub struct Context {
    time_offset: Duration,
    connector: ConnectorImpl,
    input_buffer: Vec<u8>,
}

impl Context {
    fn new(config: &Config) -> Result<Box<Context>> {
        // Here is where the time reference is recorded
        let time_offset = config.time_offset - chrono::offset::Local::now().naive_local();
        let (connector, input_buffer) = ConnectorImpl::new(&config)?;

        Ok(Box::new(Context {
                time_offset: time_offset,
                connector: connector,
                input_buffer: input_buffer,
        }))
    }
}

#[no_mangle]
pub unsafe extern fn vsg_init(argc: c_int, argv: *const *const c_char, next_arg_p: *mut c_int) -> *mut Context {
    match parse_os_args(argc, argv, |args| init(args)) {
        Ok((context, next_arg)) => {
            if let Some(next_arg_p) = next_arg_p.as_mut() {
                *next_arg_p = next_arg;
            }
            Box::into_raw(context)
        },
        Err(e) => {
            error!("vsg_init failed: {}", e);
            std::ptr::null_mut()
        },
    }
}

unsafe fn parse_os_args<F, T>(argc: c_int, argv: *const *const c_char, parse: F) -> Result<(T, c_int)>
    where F: FnOnce(&mut Iterator<Item = std::borrow::Cow<'static, std::ffi::OsStr>>) -> Result<T>
{
    use std::os::unix::ffi::OsStrExt;

    let mut next_arg = None;
    let mut args = (0..argc).filter_map(|i| {
        let str_arg = std::ffi::OsStr::from_bytes(std::ffi::CStr::from_ptr(*argv.offset(i as isize)).to_bytes());
        if next_arg.is_none() && str_arg == "--" {
            next_arg = Some(i + 1);
        }
        if next_arg.is_none() {
            Some(std::borrow::Cow::from(str_arg))
        } else {
            None
        }
    });

    match parse(&mut args) {
        Ok(r) => {
            let next_arg = match next_arg {
                Some(n) => n,
                None => argc,
            };
            Ok((r, next_arg))
        },
        Err(e) => Err(e),
    }
}

pub fn init<I>(args: I) -> Result<Box<Context>>
    where I: IntoIterator,
          I::Item: Into<std::ffi::OsString> + Clone {
    use structopt::StructOpt;

    let config = Config::from_iter_safe(args).or_else(|e| Err(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    debug!("{:?}", config);

    Context::new(&config)
}

#[no_mangle]
pub unsafe extern fn vsg_cleanup(context: *mut Context) {
    if let Some(context) = context.as_mut() {
        drop(Box::from_raw(context));
    }
}

fn gettimeofday(context: &Context) -> libc::timeval {
    let adjusted_time = chrono::offset::Local::now().naive_local() + context.time_offset;
    libc::timeval {
        tv_sec:  adjusted_time.timestamp() as libc::time_t,
        tv_usec: adjusted_time.timestamp_subsec_micros() as libc::suseconds_t,
    }
}

#[no_mangle]
pub unsafe extern fn vsg_gettimeofday(context: *const Context, timeval: *mut libc::timeval, _timezone: *mut libc::c_void) -> c_int {
    if let Some(timeval) = timeval.as_mut() {
        *timeval = gettimeofday(&*context);
    }
    0
}

fn handle_actor_msg(connector: &mut ConnectorImpl, msg: MsgIn) {
    match msg {
        MsgIn::DeliverPacket(packet) => {
        }
        MsgIn::GoToDeadline(deadline) => {
        }
    }
}

#[cfg(test)]
mod test {
    use log::{error, info};
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use stderrlog::StdErrLog;
    use super::*;
    use super::connector::test_helpers::*;

    struct OsArgs {
        raw: Vec<CString>,
        os: Vec<*const c_char>,
    }

    impl OsArgs {
        fn raw(&self) -> &[CString] {
            self.raw.as_slice()
        }

        fn argc(&self) -> c_int {
            self.raw.len() as c_int
        }

        fn argv(&self) -> *const *const c_char {
            self.os.as_ptr()
        }
    }

    macro_rules! os_args {
        ( $( $s:expr ),* ) => {{
            let mut os_args = OsArgs {
                raw: Vec::new(),
                os: Vec:: new(),
            };

            $({
                let arg = CString::new($s).unwrap();
                let ptr = arg.as_ptr();
                os_args.raw.push(arg);
                os_args.os.push(ptr);
            })*

            os_args
        }}
    }

    macro_rules! valid_args {
        () => {
            os_args!("-atiti", "-t1970-01-01T00:00:00")
        }
    }
    macro_rules! valid_args_h1 {
        () => {
            os_args!("-atiti", "-t1970-01-01T01:00:00")
        }
    }
    macro_rules! invalid_args {
        () => {
            os_args!("-btiti", "-t1970-01-01T00:00:00")
        }
    }

    static INIT: std::sync::Once = std::sync::ONCE_INIT;

    fn init() {
        // Cargo test runs all tests in a same process so don't confuse log by setting a logger
        // several times.
        INIT.call_once(|| stderrlog::new().module(module_path!()).verbosity(log::Level::Info as usize).init().unwrap());
    }

    fn parse_args_compare<I>(raw_args: &[CString], args: I, num_required_args: c_int) -> Result<()>
        where I: IntoIterator,
              I::Item: Into<std::ffi::OsString> + Clone
    {
        assert!(raw_args.len() >= num_required_args as usize);

        let mut count = 0;
        raw_args.iter().zip(args).all(|(r, a)| {
            count += 1;
            if r.as_bytes() == a.into().as_bytes() {
                true
            } else {
                false
            }
        });

        if num_required_args == count {
            Ok(())
        } else if num_required_args > count {
            Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("raw_args and args differ at index {}", count - 1)))
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("parse_os_args() iterated {} args too far", count - num_required_args)))
        }
    }

    #[test]
    // parse_os_args() correctly iterates over all args and returns the right next args index
    fn parse_os_args1() {
        let args = os_args!("-atiti", "-t1970-01-02T00:00:00");
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, 2)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(2, next);
    }

    #[test]
    // parse_os_args() correctly iterates over all args and returns the right next args index
    // no special handling of split options
    fn parse_os_args2() {
        let args = os_args!("-a", "titi", "-t", "1970-01-02T00:00:00");
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, 4)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(4, next);
    }

    #[test]
    // Next args start right after "--"
    fn parse_os_args3() {
        let args = os_args!("-atiti", "-t1970-01-02T00:00:00", "--");
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, 2)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(3, next);
    }

    #[test]
    // Next args start right after "--"
    fn parse_os_args4() {
        let args = os_args!("-atiti", "-t1970-01-02T00:00:00", "--", "other arg");
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, 2)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(3, next);
    }

    #[test]
    // Next args start right after the first occurence of "--"
    fn parse_os_args5() {
        let args = os_args!("-atiti", "-t1970-01-02T00:00:00", "--", "--");
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, 2)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(3, next);
    }

    #[test]
    fn init_valid() {
        init();

        let mut next_arg: c_int = 0;
        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), &mut next_arg) };
        assert!(!context.is_null());
        assert_eq!(2, next_arg);

        let context = unsafe { context.as_mut() }.unwrap();
        info!("context.time_offset: {:?}", context.time_offset);

        unsafe { vsg_cleanup(context) };
        test_cleanup_connect(&server_path);
    }

    #[test]
    fn init_valid_no_next_arg() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut()) };
        assert!(!context.is_null());

        let context = unsafe { context.as_mut() }.unwrap();
        info!("context.time_offset: {:?}", context.time_offset);

        unsafe { vsg_cleanup(context) };
        test_cleanup_connect(&server_path);
    }

    #[test]
    fn init_invalid() {
        init();

        let mut next_arg: c_int = 0;
        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let args = invalid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), &mut next_arg) };
        assert!(context.is_null());
        assert_eq!(0, next_arg);

        test_cleanup_connect(&server_path);
    }

    #[test]
    fn init_invalid_no_next_arg() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let args = invalid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut()) };
        assert!(context.is_null());

        test_cleanup_connect(&server_path);
    }

    #[test]
    fn cleanup_null() {
        init();

        unsafe { vsg_cleanup(std::ptr::null_mut()) }
    }

    #[test]
    fn gettimeofday() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut()) };
        assert!(!context.is_null());

        let mut tv: libc::timeval = libc::timeval {tv_sec: 0, tv_usec: 0};
        let res: c_int = unsafe { vsg_gettimeofday(context, &mut tv, std::ptr::null_mut()) };
        assert_eq!(0, res);
        // 10 seconds should be enough for slow machines...
        assert!(tv.tv_sec >= 0 && tv.tv_sec < 10);
        assert!(tv.tv_usec >= 0 && tv.tv_usec < 999999);

        unsafe { vsg_cleanup(context) };
        test_cleanup_connect(&server_path);
    }

    #[test]
    fn gettimeofday_h1() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let args = valid_args_h1!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut()) };
        assert!(!context.is_null());

        let mut tv: libc::timeval = libc::timeval {tv_sec: 0, tv_usec: 0};
        let res: c_int = unsafe { vsg_gettimeofday(context, &mut tv, std::ptr::null_mut()) };
        assert_eq!(0, res);
        // 10 seconds should be enough for slow machines...
        assert!(tv.tv_sec >= 3600 && tv.tv_sec < 3610);
        assert!(tv.tv_usec >= 0 && tv.tv_usec < 999999);

        unsafe { vsg_cleanup(context) };
        test_cleanup_connect(&server_path);
    }

    #[test]
    fn gettimeofday_no_tv() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut()) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_gettimeofday(context, std::ptr::null_mut(), std::ptr::null_mut()) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        test_cleanup_connect(&server_path);
    }

    #[test]
    fn message_loop() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut()) };
        assert!(!context.is_null());

        if let Some(context) = unsafe { context.as_mut() } {
            let connector = &mut context.connector;
            let input_buffer = &mut context.input_buffer;

            loop {
                let msg = connector.recv(input_buffer.as_mut_slice());
                match msg {
                    Ok(msg) => handle_actor_msg(connector, msg),
                    Err(e) => match e.kind() {
                        std::io::ErrorKind::Interrupted => info!("recv interrupted"),
                        _ => {
                                error!("recv failed: {}", e);
                                break;
                        },
                    },
                }
            }
        }
        unsafe { vsg_cleanup(context) };
        test_cleanup_connect(&server_path);
    }
}
