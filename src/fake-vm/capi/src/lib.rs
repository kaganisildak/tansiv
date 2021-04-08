#[cfg(test)]
#[macro_use(local_vsg_address_str, local_vsg_address, remote_vsg_address)]
extern crate tansiv_client;

use tansiv_client::{Context, Error, Result};
use libc::{self, uintptr_t};
#[allow(unused_imports)]
use log::{debug, error};
use static_assertions::const_assert;
use std::os::raw::{c_char, c_int};
use std::sync::Arc;

unsafe fn parse_os_args<F, T>(argc: c_int, argv: *const *const c_char, parse: F) -> Result<(T, c_int)>
    where F: FnOnce(&mut dyn Iterator<Item = std::borrow::Cow<'static, std::ffi::OsStr>>) -> Result<T>
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

type CRecvCallback = unsafe extern "C" fn(uintptr_t);

#[no_mangle]
pub unsafe extern fn vsg_init(argc: c_int, argv: *const *const c_char, next_arg_p: *mut c_int, recv_callback: CRecvCallback, recv_callback_arg: uintptr_t) -> *const Context {
    let callback: tansiv_client::RecvCallback = Box::new(move || recv_callback(recv_callback_arg));

    match parse_os_args(argc, argv, |args| tansiv_client::init(args, callback)) {
        Ok((context, next_arg)) => {
            if let Some(next_arg_p) = next_arg_p.as_mut() {
                *next_arg_p = next_arg;
            }
            Arc::into_raw(context)
        },
        Err(e) => {
            error!("vsg_init failed: {}", e);
            std::ptr::null_mut()
        },
    }
}

#[no_mangle]
pub unsafe extern fn vsg_cleanup(context: *const Context) {
    if !context.is_null() {
        drop(Arc::from_raw(context));
    }
}

/// Start the simulation and optionally return the time offset from simulation time to execution
/// context time.
///
/// The simulation time is assumed to start at 0 and the execution context time anchor depends on
/// the context type:
/// - for a process execution context, Time 0 refers to UNIX Epoch, that is
///   1970/01/01 00:00;
/// - for a Qemu execution context, Time 0 is set internally in Qemu and the offset is recorded
///   when calling vsg_start().
///
/// # Safety
///
/// * `context` should point to a valid context, as previously returned by [`vsg_init`].
///
/// * `offset` may be `NULL` or should point to a valid memory area.
///
/// # Error codes
///
/// * Fails with `libc::EINVAL` whenever context is `NULL`.
///
/// * Fails with `libc::EALREADY` whenever start() has already been called on this context.
///
/// * Fails with `libc::ENOMEM` if no buffer can be allocated for vsg protocol messages.
///
/// * Fails with `libc::EPROTO` if an error occurs in the vsg protocol.
///
/// * Fails with `libc::EMSGSIZE` if message buffers were configured too short for vsg protocol
///   messages.
///
/// * Fails with `libc::EIO` if any other error happens in the low-level communication functions of
///   the vsg protocol.
#[no_mangle]
pub unsafe extern fn vsg_start(context: *const Context, offset: *mut libc::timespec) -> c_int {
    if let Some(context) = context.as_ref() {
        match (*context).start() {
            Ok(o) => {
                let num_seconds = o.num_seconds();
                let num_subsec_nanos = (o - chrono::Duration::seconds(num_seconds)).num_nanoseconds().unwrap();
                if let Some(offset) = offset.as_mut() {
                    *offset = libc::timespec {
                        tv_sec: num_seconds as libc::time_t,
                        tv_nsec: num_subsec_nanos as libc::c_long,
                    };
                }
                0
            },
            Err(e) => match e {
                Error::AlreadyStarted => libc::EALREADY,
                Error::NoMemoryAvailable => libc::ENOMEM,
                Error::ProtocolViolation => libc::EPROTO,
                Error::SizeTooBig => libc::EMSGSIZE,
                _ => // Unknown error, fallback to EIO
                    libc::EIO,
            },
        }
    } else {
        libc::EINVAL
    }
}

/// Stop the simulation.
///
/// # Safety
///
/// * `context` should point to a valid context, as previously returned by [`vsg_init`].
///
/// # Error codes
///
/// * Fails with `libc::EINVAL` whenever context is `NULL`.
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

/// Sends a message having source address `src`, destination address `dst` and a payload stored in
/// `msg[0..msglen]`.
///
/// # Safety
///
/// * `context` should point to a valid context, as previously returned by [`vsg_init`].
///
/// * If `msglen` is `0`, it is allowed that `msg` is `NULL`.
///
/// # Error codes
///
/// * Fails with `libc::EINVAL` whenever context is `NULL` or `msg` is `NULL` with `msglen > 0`.
///
/// * Fails with `libc::EMSGSIZE` whenever the payload is bigger than the maximum message size that
///   vsg can handle.
///
/// * Fails with `libc::ENOMEM` whenever there is no more buffers to hold the message to send.
#[no_mangle]
pub unsafe extern fn vsg_send(context: *const Context, dst: libc::in_addr_t, msglen: u32, msg: *const u8) -> c_int {
    if let Some(context) = context.as_ref() {
        // We can tolerate msg.is_null() if msglen == 0 but std::slice::from_raw_parts() requires
        // non null pointers.
        let ptr = if msglen == 0 {
            std::ptr::NonNull::dangling().as_ptr()
        } else {
            if msg.is_null() {
                return libc::EINVAL;
            };
            msg
        };
        let payload = std::slice::from_raw_parts(ptr, msglen as usize);

        match (*context).send(dst, payload) {
            Ok(_) => 0,
            Err(e) => match e {
                Error::NoMemoryAvailable => libc::ENOMEM,
                Error::SizeTooBig => libc::EMSGSIZE,
                _ => // Unknown error, fallback to EIO
                    libc::EIO,
            },
        }
    } else {
        libc::EINVAL
    }
}

/// Picks the next message in the receive queue, stores its payload in `msg[0..*msglen]` and
/// optionnally returns sender and destination addresses in `*psrc` and `*pdst` respectively.
/// `*msglen` initially contains the size of the buffer pointed to by `msg`. When `vsg_recv`
/// returns with success, `*msglen` contains the actual length of the received payload.
///
/// # Safety
///
/// * `context` should point to a valid context, as previously returned by [`vsg_init`].
///
/// * `psrc` and `pdst` can be `NULL`, in which case the correponding addresses will not be returned.
///
/// * If `msglen` is not `NULL`, `msg` must point to a valid memory range of at least `*msglen`
///   bytes. This memory range does not need to be initialized.
///
/// * If `msglen` is `NULL` or `*msglen` is `0`, only 0-length messages can be received. Note that
///   in that case it is allowed that `msg` is `NULL` too.
///
/// # Error codes
///
/// * Fails with `libc::EINVAL` whenever the pointers in arguments do not respect the rules above.
///
/// * Fails with `libc::EAGAIN` if the receive queue was empty.
///
/// * Fails with `libc::EMSGSIZE` whenever the next message in the queue has a payload bigger than
///   the provided buffer. The message is lost.
#[no_mangle]
pub unsafe extern fn vsg_recv(context: *const Context, psrc: *mut libc::in_addr_t, pdst: *mut libc::in_addr_t, msglen: *mut u32, msg: *mut u8) -> c_int {
    const_assert!(tansiv_client::MAX_PACKET_SIZE <= std::u32::MAX as usize);

    if let Some(context) = context.as_ref() {
        let len = if msglen.is_null() {
            0
        } else {
            *msglen
        };
        // We can tolerate msg.is_null() if len == 0 but std::slice::from_raw_parts_mut() requires
        // non null pointers.
        let ptr = if len == 0 {
            std::ptr::NonNull::dangling().as_ptr()
        } else {
            if msg.is_null() {
                return libc::EINVAL;
            };
            msg
        };
        let payload = std::slice::from_raw_parts_mut(ptr, len as usize);

        match (*context).recv(payload) {
            Ok((src, dst, payload)) => {
                if !psrc.is_null() {
                    *psrc = src;
                }
                if !pdst.is_null() {
                    *pdst = dst;
                }
                if !msglen.is_null() {
                    *msglen = payload.len() as u32;
                }
                0
            },
            Err(e) => match e {
                Error::NoMessageAvailable => libc::EAGAIN,
                Error::SizeTooBig => libc::EMSGSIZE,
                _ => // Unknown error, fallback to EIO
                    libc::EIO,
            },
        }
    } else {
        libc::EINVAL
    }
}

/// Checks if a message can be read from the input queue. If `0` is returned a message can be read
/// from the input queue using [`vsg_recv`].
///
/// # Safety
///
/// * `context` should point to a valid context, as previously returned by [`vsg_init`].
///
/// # Error codes
///
/// * Fails with `libc::EINVAL` whenever `context` is NULL.
///
/// * Fails with `libc::EAGAIN` if the receive queue was empty.
#[no_mangle]
pub unsafe extern fn vsg_poll(context: *const Context) -> c_int {
    if let Some(context) = context.as_ref() {
        match (*context).poll() {
            Some(_) => 0,
            None => libc::EAGAIN,
        }
    } else {
        libc::EINVAL
    }
}

#[cfg(test)]
mod test {
    use tansiv_client::test_helpers::*;
    use libc::{timespec, timeval};
    #[allow(unused_imports)]
    use log::{error, info};
    use std::ffi::CString;
    use std::pin::Pin;
    use std::os::raw::{c_char, c_int};
    use std::os::unix::ffi::OsStrExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use super::*;

    macro_rules! null_vsg_address {
        () => {
            0u32
        }
    }

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
            os_args!("-atiti", "-n", local_vsg_address_str!(), "-t1970-01-01T00:00:00")
        }
    }
    macro_rules! invalid_args {
        () => {
            os_args!("-btiti", "-n", local_vsg_address_str!(), "-t1970-01-01T00:00:00")
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
        let args = os_args!("-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00");
        let num_required_args = args.argc();
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, num_required_args)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(num_required_args, next);
    }

    #[test]
    // parse_os_args() correctly iterates over all args and returns the right next args index
    // no special handling of split options
    fn parse_os_args2() {
        let args = os_args!("-a", "titi", "-n", "10.0.0.1", "-t", "1970-01-02T00:00:00");
        let num_required_args = args.argc();
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, num_required_args)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(num_required_args, next);
    }

    #[test]
    // Next args start right after "--"
    fn parse_os_args3() {
        let args = os_args!("-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00", "--");
        let num_required_args = args.argc() - 1;
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, num_required_args)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(num_required_args + 1, next);
    }

    #[test]
    // Next args start right after "--"
    fn parse_os_args4() {
        let args = os_args!("-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00", "--", "other arg");
        let num_required_args = args.argc() - 2;
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, num_required_args)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(num_required_args + 1, next);
    }

    #[test]
    // Next args start right after the first occurence of "--"
    fn parse_os_args5() {
        let args = os_args!("-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00", "--", "--");
        let num_required_args = args.argc() - 2;
        let res = unsafe { parse_os_args(args.argc(), args.argv(), |a| parse_args_compare(args.raw(), a, num_required_args)) };
        assert!(res.is_ok());
        let (_, next) = res.unwrap();
        assert_eq!(num_required_args + 1, next);
    }

    extern "C" fn dummy_recv_callback(_arg: uintptr_t) -> () {
    }

    // C-style version of tansiv_client::test_helpers::RecvNotifier
    // Ugly
    struct RecvNotifier(AtomicBool);

    impl RecvNotifier {
        fn new() -> RecvNotifier {
            RecvNotifier(AtomicBool::new(false))
        }

        fn notify(&self) -> () {
            self.0.store(true, Ordering::SeqCst);
        }

        fn wait(&self, pause_slice_micros: u64) -> () {
            while !self.0.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_micros(pause_slice_micros));
            }
            self.0.store(false, Ordering::SeqCst);
        }

        fn pin(&self) -> Pin<&Self> {
            Pin::new(self)
        }

        fn get_callback_arg(pinned: &Pin<&Self>) -> uintptr_t {
            use std::ops::Deref;
            pinned.deref() as *const Self as uintptr_t
        }

        unsafe extern "C" fn callback(arg: uintptr_t) -> () {
            let pinned = (arg as *const Self).as_ref().unwrap();
            pinned.notify()
        }
    }

    #[test]
    fn init_valid() {
        init();

        let mut next_arg: c_int = 0;
        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), &mut next_arg, dummy_recv_callback, 0) };
        assert!(!context.is_null());
        assert_eq!(args.argc(), next_arg);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn init_valid_no_next_arg() {
        init();

        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(!context.is_null());

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn init_invalid() {
        init();

        let mut next_arg: c_int = 0;
        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let args = invalid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), &mut next_arg, dummy_recv_callback, 0) };
        assert!(context.is_null());
        assert_eq!(0, next_arg);

        drop(actor);
    }

    #[test]
    fn init_invalid_no_next_arg() {
        init();

        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let args = invalid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(context.is_null());

        drop(actor);
    }

    #[test]
    fn cleanup_null() {
        init();

        unsafe { vsg_cleanup(std::ptr::null_mut()) }
    }

    const TIMESPEC_POISON: timespec = timespec {
        tv_sec: std::i64::MAX,
        tv_nsec: std::i64::MAX,
    };

    const TIMEVAL_POISON: timeval = timeval {
        tv_sec: std::i64::MAX,
        tv_usec: std::i64::MAX,
    };

    #[test]
    fn start_stop() {
        init();

        let actor = TestActorDesc::new("titi", start_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(!context.is_null());

        let mut offset = TIMESPEC_POISON;
        let res: c_int = unsafe { vsg_start(context, &mut offset) };
        assert_eq!(0, res);
        assert_eq!(0, offset.tv_sec);
        assert_eq!(0, offset.tv_nsec);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn start_no_context() {
        init();

        let res: c_int = unsafe { vsg_start(std::ptr::null(), std::ptr::null_mut()) };
        assert_eq!(libc::EINVAL, res);
    }

    #[test]
    fn start_no_offset() {
        init();

        let actor = TestActorDesc::new("titi", start_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
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

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        let buffer = b"Foo msg";
        let dst = remote_vsg_address!();
        let res: c_int = unsafe { vsg_send(context, dst, buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn send_null_empty() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        let dst = remote_vsg_address!();
        let res: c_int = unsafe { vsg_send(context, dst, 0, std::ptr::null()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn send_null_not_empty() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        let dst = remote_vsg_address!();
        let res: c_int = unsafe { vsg_send(context, dst, 1, std::ptr::null()) };
        assert_eq!(libc::EINVAL, res);

        // Terminate gracefully
        let buffer = b"Foo msg";
        let res: c_int = unsafe { vsg_send(context, dst, buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn send_too_big() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        let buffer = [0u8; tansiv_client::MAX_PACKET_SIZE + 1];
        let dst = remote_vsg_address!();
        let res: c_int = unsafe { vsg_send(context, dst, buffer.len() as u32, (&buffer).as_ptr()) };
        assert_eq!(libc::EMSGSIZE, res);

        // Terminate gracefully
        let buffer = b"Foo msg";
        let res: c_int = unsafe { vsg_send(context, dst, buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn send_no_context() {
        init();

        let buffer =  b"Foo msg";
        let dst = remote_vsg_address!();
        let res: c_int = unsafe { vsg_send(std::ptr::null(), dst, buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(libc::EINVAL, res);
    }

    #[test]
    fn recv() {
        init();

        const EXPECTED_MSG: &[u8] = b"Foo msg";
        let mut buffer = [0u8; EXPECTED_MSG.len()];

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        recv_notifier.wait(1000);

        let mut src = null_vsg_address!();
        let mut dst = null_vsg_address!();
        let mut buffer_len: u32 = buffer.len() as u32;
        let res: c_int = unsafe { vsg_recv(context, &mut src, &mut dst, &mut buffer_len, buffer.as_mut().as_mut_ptr()) };
        assert_eq!(0, res);

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer_len, EXPECTED_MSG.len() as u32);
        assert_eq!(buffer, EXPECTED_MSG);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn recv_no_src() {
        init();

        const EXPECTED_MSG: &[u8] = b"Foo msg";
        let mut buffer = [0u8; EXPECTED_MSG.len()];

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        recv_notifier.wait(1000);

        let mut dst = null_vsg_address!();
        let mut buffer_len: u32 = buffer.len() as u32;
        let res: c_int = unsafe { vsg_recv(context, std::ptr::null_mut(), &mut dst, &mut buffer_len, buffer.as_mut().as_mut_ptr()) };
        assert_eq!(0, res);

        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer_len, EXPECTED_MSG.len() as u32);
        assert_eq!(buffer, EXPECTED_MSG);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn recv_no_dst() {
        init();

        const EXPECTED_MSG: &[u8] = b"Foo msg";
        let mut buffer = [0u8; EXPECTED_MSG.len()];

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        recv_notifier.wait(1000);

        let mut src = null_vsg_address!();
        let mut buffer_len: u32 = buffer.len() as u32;
        let res: c_int = unsafe { vsg_recv(context, &mut src, std::ptr::null_mut(), &mut buffer_len, buffer.as_mut().as_mut_ptr()) };
        assert_eq!(0, res);

        // FIXME: Use the remote address when available in the protocol
        assert_eq!(src, local_vsg_address!());
        assert_eq!(buffer_len, EXPECTED_MSG.len() as u32);
        assert_eq!(buffer, EXPECTED_MSG);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn recv_null_empty() {
        init();

        const EXPECTED_MSG: &[u8] = b"";

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        recv_notifier.wait(1000);

        let mut src = null_vsg_address!();
        let mut dst = null_vsg_address!();
        let res: c_int = unsafe { vsg_recv(context, &mut src, &mut dst, std::ptr::null_mut(), std::ptr::null_mut()) };
        assert_eq!(0, res);

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn recv_null_empty2() {
        init();

        const EXPECTED_MSG: &[u8] = b"";

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        recv_notifier.wait(1000);

        let mut src = null_vsg_address!();
        let mut dst = null_vsg_address!();
        let mut buffer_len = 0u32;
        let res: c_int = unsafe { vsg_recv(context, &mut src, &mut dst, &mut buffer_len, std::ptr::null_mut()) };
        assert_eq!(0, res);

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer_len, 0);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn recv_null_empty3() {
        init();

        const EXPECTED_MSG: &[u8] = b"";

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        recv_notifier.wait(1000);

        let mut src = null_vsg_address!();
        let mut dst = null_vsg_address!();
        let mut buffer: [u8; 0] = [];
        let res: c_int = unsafe { vsg_recv(context, &mut src, &mut dst, std::ptr::null_mut(), buffer.as_mut().as_mut_ptr()) };
        assert_eq!(0, res);

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer, EXPECTED_MSG);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn recv_no_context() {
        init();

        const ORIG_BUFFER: &[u8] = b"Foo msg";
        let mut buffer: [u8; ORIG_BUFFER.len()] = Default::default();
        buffer.copy_from_slice(ORIG_BUFFER);

        let mut buffer_len: u32 = buffer.len() as u32;
        let mut src = local_vsg_address!();
        let mut dst = remote_vsg_address!();
        let res: c_int = unsafe { vsg_recv(std::ptr::null(), &mut src, &mut dst, &mut buffer_len, buffer.as_mut().as_mut_ptr()) };
        assert_eq!(libc::EINVAL, res);
        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer_len, ORIG_BUFFER.len() as u32);
        assert_eq!(buffer, ORIG_BUFFER);
    }

    #[test]
    fn recv_null_not_empty() {
        init();

        const EXPECTED_MSG: &[u8] = b"";

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        recv_notifier.wait(1000);

        let mut src = local_vsg_address!();
        let mut dst = remote_vsg_address!();
        let mut buffer_len = 1u32;
        let res: c_int = unsafe { vsg_recv(context, &mut src, &mut dst, &mut buffer_len, std::ptr::null_mut()) };
        assert_eq!(libc::EINVAL, res);

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer_len, 1);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn recv_no_msg() {
        init();

        const ORIG_BUFFER: &[u8] = b"Foo msg";
        let mut buffer: [u8; ORIG_BUFFER.len()] = Default::default();
        buffer.copy_from_slice(ORIG_BUFFER);

        let actor = TestActorDesc::new("titi", start_actor);

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        let mut src = local_vsg_address!();
        let mut dst = remote_vsg_address!();
        let mut buffer_len: u32 = buffer.len() as u32;
        let res: c_int = unsafe { vsg_recv(context, &mut src, &mut dst, &mut buffer_len, buffer.as_mut().as_mut_ptr()) };
        assert_eq!(libc::EAGAIN, res);

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer_len, ORIG_BUFFER.len() as u32);
        assert_eq!(buffer, ORIG_BUFFER);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn recv_too_big() {
        init();

        const SENT_MSG: &[u8] = b"Foo msg";
        const ORIG_BUFFER: &[u8] = b"fOO MS";
        let mut buffer: [u8; ORIG_BUFFER.len()] = Default::default();
        buffer.copy_from_slice(ORIG_BUFFER);

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, SENT_MSG));

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        recv_notifier.wait(1000);

        let mut src = local_vsg_address!();
        let mut dst = remote_vsg_address!();
        let mut buffer_len: u32 = buffer.len() as u32;
        let res: c_int = unsafe { vsg_recv(context, &mut src, &mut dst, &mut buffer_len, buffer.as_mut().as_mut_ptr()) };
        assert_eq!(libc::EMSGSIZE, res);

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer_len, ORIG_BUFFER.len() as u32);
        assert_eq!(buffer, ORIG_BUFFER);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn poll() {
        init();

        const EXPECTED_MSG: &[u8] = b"Foo msg";
        let mut buffer = [0u8; EXPECTED_MSG.len()];

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let args = valid_args!();
        let recv_notifier = RecvNotifier::new();
        let recv_notifier = recv_notifier.pin();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), RecvNotifier::callback, RecvNotifier::get_callback_arg(&recv_notifier)) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        loop {
            match unsafe { vsg_poll(context) } {
               libc::EAGAIN => std::thread::sleep(std::time::Duration::from_micros(1000)),
               res => {
                   assert_eq!(0, res);
                   break;
               },
            }
        }

        let mut src = null_vsg_address!();
        let mut dst = null_vsg_address!();
        let mut buffer_len: u32 = buffer.len() as u32;
        let res: c_int = unsafe { vsg_recv(context, &mut src, &mut dst, &mut buffer_len, buffer.as_mut().as_mut_ptr()) };
        assert_eq!(0, res);

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer_len, EXPECTED_MSG.len() as u32);
        assert_eq!(buffer, EXPECTED_MSG);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn gettimeofday() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        let mut tv = TIMEVAL_POISON;
        let res: c_int = unsafe { vsg_gettimeofday(context, &mut tv, std::ptr::null_mut()) };
        assert_eq!(0, res);
        assert_ne!(TIMEVAL_POISON.tv_sec, tv.tv_sec);
        assert_ne!(TIMEVAL_POISON.tv_usec, tv.tv_usec);

        let buffer = b"This is the end";
        let dst = remote_vsg_address!();
        let res: c_int = unsafe { vsg_send(context, dst, buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn gettimeofday_no_tv() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let args = valid_args!();
        let context = unsafe { vsg_init(args.argc(), args.argv(), std::ptr::null_mut(), dummy_recv_callback, 0) };
        assert!(!context.is_null());

        let res: c_int = unsafe { vsg_start(context, std::ptr::null_mut()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_gettimeofday(context, std::ptr::null_mut(), std::ptr::null_mut()) };
        assert_eq!(0, res);

        let buffer = b"This is the end";
        let dst = remote_vsg_address!();
        let res: c_int = unsafe { vsg_send(context, dst,  buffer.len() as u32, buffer.as_ref().as_ptr()) };
        assert_eq!(0, res);

        let res: c_int = unsafe { vsg_stop(context) };
        assert_eq!(0, res);

        unsafe { vsg_cleanup(context) };
        drop(actor);
    }

    #[test]
    fn gettimeofday_no_context() {
        init();

        let mut tv = TIMEVAL_POISON;
        let res: c_int = unsafe { vsg_gettimeofday(std::ptr::null(), &mut tv, std::ptr::null_mut()) };
        assert_eq!(libc::EINVAL, res);
        assert_eq!(TIMEVAL_POISON.tv_sec, tv.tv_sec);
        assert_eq!(TIMEVAL_POISON.tv_usec, tv.tv_usec);
    }
}
