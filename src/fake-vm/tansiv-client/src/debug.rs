macro_rules! deadline_handler_debug {
    ($($arg:tt)*) => {
        if cfg!(feature = "deadline-handler-debug") {
            ::log::debug!($($arg)*)
        }
    }
}
