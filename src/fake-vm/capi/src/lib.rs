use fake_vm::{Context, Error, Result};
use libc;
#[allow(unused_imports)]
use log::{debug, error};
use static_assertions::const_assert;
use std::os::raw::{c_char, c_int};

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

type CRecvCallback = extern "C" fn(*const Context, u32, *const u8);

#[no_mangle]
pub unsafe extern fn vsg_init(argc: c_int, argv: *const *const c_char, next_arg_p: *mut c_int, recv_callback: CRecvCallback) -> *mut Context {
    const_assert!(fake_vm::MAX_PACKET_SIZE <= std::u32::MAX as usize);
    let callback: fake_vm::RecvCallback = Box::new(move |context, payload| recv_callback(context, payload.len() as u32, payload.as_ptr()));

    match parse_os_args(argc, argv, |args| fake_vm::init(args, callback)) {
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

#[no_mangle]
pub unsafe extern fn vsg_cleanup(context: *mut Context) {
    if let Some(context) = context.as_mut() {
        drop(Box::from_raw(context));
    }
}

#[no_mangle]
pub unsafe extern fn vsg_start(context: *const Context) -> c_int {
    if let Some(context) = context.as_ref() {
        match (*context).start() {
            Ok(_) => 0,
            Err(e) => match e {
                Error::AlreadyStarted => libc::EALREADY,
                Error::NoMemoryAvailable => libc::ENOMEM,
                Error::ProtocolViolation => libc::EPROTO,
                Error::SizeTooBig => libc::E2BIG,
                _ => // Unknown error, fallback to EIO
                    libc::EIO,
            },
        }
    } else {
        libc::EINVAL
    }
}

#[no_mangle]
pub unsafe extern fn vsg_stop(context: *const Context) -> c_int {
    if let Some(context) = context.as_ref() {
        (*context).stop();
        0
    } else {
        libc::EINVAL
    }
}

#[no_mangle]
pub unsafe extern fn vsg_gettimeofday(context: *const Context, timeval: *mut libc::timeval, _timezone: *mut libc::c_void) -> c_int {
    if let Some(context) = context.as_ref() {
        if let Some(timeval) = timeval.as_mut() {
            *timeval = context.gettimeofday();
        }
        0
    } else {
        libc::EINVAL
    }
}

#[no_mangle]
pub unsafe extern fn vsg_send(context: *const Context, msglen: u32, msg: *const u8) -> c_int {
    if let Some(context) = context.as_ref() {
        let payload = std::slice::from_raw_parts(msg, msglen as usize);

        match (*context).send(payload) {
            Ok(_) => 0,
            Err(e) => match e {
                Error::NoMemoryAvailable => libc::ENOMEM,
                Error::SizeTooBig => libc::E2BIG,
                _ => // Unknown error, fallback to EIO
                    libc::EIO,
            },
        }
    } else {
        libc::EINVAL
    }
}

#[cfg(test)]
mod test {
    use fake_vm::test_helpers::*;
    use libc::timeval;
    #[allow(unused_imports)]
    use log::{error, info};
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};
    use std::os::unix::ffi::OsStrExt;
    use super::*;

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
    macro_rules! invalid_args {
        () => {
            os_args!("-btiti", "-t1970-01-01T00:00:00")
        }
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
            Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("raw_args and args differ at index {}", count - 1)).into())
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("parse_os_args() iterated {} args too far", count - num_required_args)).into())
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

    extern "C" fn dummy_recv_callback(_context: *const Context, _packet_len: u32, _packet: *const u8) -> () {
    }

    #[test]
    fn init_valid() {
        init();

        let mut next_arg: c_int = 0;
        let actor = TestActor::new("titi", TestActor::dummy_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), &mut next_arg, dummy_recv_callback) };
        assert!(!context.is_null());
        assert_eq!(2, next_arg);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn init_valid_no_next_arg() {
        init();

        let actor = TestActor::new("titi", TestActor::dummy_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback) };
        assert!(!context.is_null());

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn init_invalid() {
        init();

        let mut next_arg: c_int = 0;
        let actor = TestActor::new("titi", TestActor::dummy_actor);
        let args = invalid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), &mut next_arg, dummy_recv_callback) };
        assert!(context.is_null());
        assert_eq!(0, next_arg);

        drop(actor);
    }

    #[test]
    fn init_invalid_no_next_arg() {
        init();

        let actor = TestActor::new("titi", TestActor::dummy_actor);
        let args = invalid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback) };
        assert!(context.is_null());

        drop(actor);
    }

    #[test]
    fn cleanup_null() {
        init();

        unsafe { vsg_cleanup(std::ptr::null_mut()) }
    }

    const TIMEVAL_POISON: timeval = timeval {
        tv_sec: std::i64::MAX,
        tv_usec: std::i64::MAX,
    };

    #[test]
    fn start_stop() {
        init();

        let actor = TestActor::new("titi", start_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn start_no_context() {
        init();

        let res: c_int = unsafe { vsg_start(std::ptr::null()) };
        assert_eq!(libc::EINVAL, res);
    }

    #[test]
    fn stop_no_context() {
        init();

        let res: c_int = unsafe { vsg_stop(std::ptr::null()) };
        assert_eq!(libc::EINVAL, res);
    }

    #[test]
    fn send() {
        init();

        let actor = TestActor::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context) };
        assert_eq!(0, res);

        let buffer = b"Foo msg";
        let res: c_int = unsafe { vsg_send(context, buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn send_too_big() {
        init();

        let actor = TestActor::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context) };
        assert_eq!(0, res);

        let buffer = [0u8; fake_vm::MAX_PACKET_SIZE + 1];
        let res: c_int = unsafe { vsg_send(context, buffer.len() as u32, (&buffer).as_ptr()) };
        assert_eq!(libc::E2BIG, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn send_no_context() {
        init();

        let buffer =  b"Foo msg";
        let res: c_int = unsafe { vsg_send(std::ptr::null(), buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(libc::EINVAL, res);
    }

    #[test]
    fn gettimeofday() {
        init();

        let actor = TestActor::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context) };
        assert_eq!(0, res);

        let mut tv = TIMEVAL_POISON;
        let res: c_int = unsafe { vsg_gettimeofday(context, &mut tv, std::ptr::null_mut()) };
        assert_eq!(0, res);
        assert_ne!(TIMEVAL_POISON.tv_sec, tv.tv_sec);
        assert_ne!(TIMEVAL_POISON.tv_usec, tv.tv_usec);

        let buffer = b"This is the end";
        let res: c_int = unsafe { vsg_send(context, buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn gettimeofday_no_tv() {
        init();

        let actor = TestActor::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_gettimeofday(context, std::ptr::null_mut(), std::ptr::null_mut()) };
        assert_eq!(0, res);

        let buffer = b"This is the end";
        let res: c_int = unsafe { vsg_send(context, buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn gettimeofday_no_context() {
        init();

        let mut tv = TIMEVAL_POISON;;
        let res: c_int = unsafe { vsg_gettimeofday(std::ptr::null(), &mut tv, std::ptr::null_mut()) };
        assert_eq!(libc::EINVAL, res);
        assert_eq!(TIMEVAL_POISON.tv_sec, tv.tv_sec);
        assert_eq!(TIMEVAL_POISON.tv_usec, tv.tv_usec);
    }
}
