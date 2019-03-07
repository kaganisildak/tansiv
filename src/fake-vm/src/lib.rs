#![feature(try_from)]
#![feature(uniform_paths)]
// Use chrono::Duration (re-exported from time::Duration) to allow negative values, which are not
// allowed in std::time::Duration
use chrono::{Duration, naive::NaiveDateTime};
pub(crate) use config::Config;
use connector::{Connector, ConnectorImpl, MsgIn, MsgOut};
use libc;
use log::{debug, error};
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
    fn new(config: &Config) -> std::io::Result<Box<Context>> {
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
pub unsafe extern fn vsg_init(argc: c_int, argv: *const *const c_char, next_arg: *mut c_int) -> *mut Context {
    let (config, next) = match Config::from_os_args(argc, argv) {
        Ok(r) => r,
        Err(e) => {
            error!("vsg_init failed: {}", e);
            return std::ptr::null_mut();
        },
    };
    debug!("{:?}", config);

    match Context::new(&config) {
        Ok(context) => {
            if let Some(next_arg) = next_arg.as_mut() {
                *next_arg = next;
            }
            Box::into_raw(context)
        },
        Err(e) => {
            error!("vsg_init failed: {}", e);
            std::ptr::null_mut()
        }
    }
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
#[macro_export]
macro_rules! os_args {
    ( $( $s:expr ),* ) => {
        [$(std::ffi::CString::new($s).unwrap().as_ptr(),)*].as_ptr()
    }
}

#[cfg(test)]
mod test {
    use log::{error, info};
    use std::path::PathBuf;
    use std::os::raw::c_int;
    use stderrlog::StdErrLog;
    use super::*;
    use super::connector::test_helpers::*;

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

    #[test]
    fn init_valid() {
        init();

        let mut next_arg: c_int = 0;
        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let context = unsafe { vsg_init(2, valid_args!(), &mut next_arg) };
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
        let context = unsafe { vsg_init(2, valid_args!(), std::ptr::null_mut()) };
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
        let context = unsafe { vsg_init(2, invalid_args!(), &mut next_arg) };
        assert!(context.is_null());
        assert_eq!(0, next_arg);

        test_cleanup_connect(&server_path);
    }

    #[test]
    fn init_invalid_no_next_arg() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let context = unsafe { vsg_init(2, invalid_args!(), std::ptr::null_mut()) };
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
        let context = unsafe { vsg_init(2, valid_args!(), std::ptr::null_mut()) };
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
        let context = unsafe { vsg_init(2, valid_args_h1!(), std::ptr::null_mut()) };
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
        let context = unsafe { vsg_init(2, valid_args!(), std::ptr::null_mut()) };
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
        let context = unsafe { vsg_init(2, valid_args!(), std::ptr::null_mut()) };
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
