use chrono::naive::NaiveDateTime;
use std::os::raw::{c_char, c_int};
use std::str::FromStr;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "libfake-vm", raw(setting = "structopt::clap::AppSettings::NoBinaryName"))]
pub(crate) struct Config {
    /// Server socket address of the actor (only UNIX sockets supported)
    #[structopt(short = "a", long = "actor", parse(from_os_str))]
    pub actor_socket: std::path::PathBuf,

    /// Initial time in the VM, formatted as %Y-%m-%dT%H:%M:%S%.f (%.f part is optional)
    #[structopt(short = "t", long = "initial_time", parse(try_from_str = "chrono::naive::NaiveDateTime::from_str"))]
    pub time_offset: NaiveDateTime,
}

impl Config {
    pub(super) unsafe fn from_os_args(argc: c_int, argv: *const *const c_char) -> Result<(Config, c_int), structopt::clap::Error> {
        use std::os::unix::ffi::OsStrExt;

        let mut next_arg: c_int = 0;
        let args = (0..argc).filter_map(|i| {
            let str_arg = std::ffi::OsStr::from_bytes(std::ffi::CStr::from_ptr(*argv.offset(i as isize)).to_bytes());
            if next_arg == 0 && str_arg == "--" {
                next_arg = (i + 1) as c_int;
            }
            if next_arg == 0 {
                Some(std::borrow::Cow::from(str_arg))
            } else {
                None
            }
        });

        let config = Config::from_iter_safe(args)?;
        if next_arg == 0 {
            next_arg = argc;
        }
        Ok((config, next_arg))
    }
}

#[cfg(test)]
mod test {
    use crate::os_args;
    use super::*;

    #[test]
    // Correct args and next args index in case of sticked options and values
    fn valid_args1() {
        let (config, next) = unsafe { Config::from_os_args(2, os_args!("-atiti", "-t1970-01-02T00:00:00")) }.unwrap();
        assert_eq!(2, next);

        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
    }

    #[test]
    // Correct args and next args index in case of split options and values
    fn valid_args2() {
        let (config, next) = unsafe { Config::from_os_args(4, os_args!("-a", "titi", "-t", "1970-01-02T00:00:00")) }.unwrap();
        assert_eq!(4, next);

        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
    }

    #[test]
    // Next args start right after "--"
    fn valid_args3() {
        let (_config, next) = unsafe { Config::from_os_args(3, os_args!("-atiti", "-t1970-01-02T00:00:00", "--")) }.unwrap();
        assert_eq!(3, next);
    }

    #[test]
    // Next args start right after "--"
    fn valid_args4() {
        let (_config, next) = unsafe { Config::from_os_args(4, os_args!("-atiti", "-t1970-01-02T00:00:00", "--", "other arg")) }.unwrap();
        assert_eq!(3, next);
    }

    #[test]
    // Next args start right after the first occurence of "--"
    fn valid_args5() {
        let (_config, next) = unsafe { Config::from_os_args(4, os_args!("-atiti", "-t1970-01-02T00:00:00", "--", "--")) }.unwrap();
        assert_eq!(3, next);
    }

    #[test]
    // Missing socket value
    fn invalid_args1() {
        assert!(unsafe { Config::from_os_args(2, os_args!("-a", "-t1970-01-02T00:00:00")) }.is_err());
    }

    #[test]
    // Missing time value
    fn invalid_args2() {
        assert!(unsafe { Config::from_os_args(3, os_args!("-a", "titi", "-t")) }.is_err());
    }

    #[test]
    // Missing time
    fn invalid_args3() {
        assert!(unsafe { Config::from_os_args(2, os_args!("-a", "titi")) }.is_err());
    }

    #[test]
    // Missing actor_socket
    fn invalid_args4() {
        assert!(unsafe { Config::from_os_args(1, os_args!("-t1970-01-02T00:00:00")) }.is_err());
    }

    #[test]
    // Invalid time format
    fn invalid_args5() {
        assert!(unsafe { Config::from_os_args(2, os_args!("-atiti", "-t1970-01-02T00:00")) }.is_err());
    }
}
