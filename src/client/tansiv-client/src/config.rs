use chrono::naive::NaiveDateTime;
use libc;
use std::num::NonZeroUsize;
use std::str::FromStr;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "libtansiv-client", raw(setting = "structopt::clap::AppSettings::NoBinaryName"))]
pub(crate) struct Config {
    /// Server socket address of the actor (only UNIX sockets supported)
    #[structopt(short = "a", long = "actor", parse(from_os_str))]
    pub actor_socket: std::path::PathBuf,

    /// Name (address) of this application in the network
    #[structopt(short = "n", long = "name", parse(try_from_str = "crate::vsg_address::from_str"))]
    pub address: libc::in_addr_t,

    /// Initial time in the VM, formatted as %Y-%m-%dT%H:%M:%S%.f (%.f part is optional)
    #[structopt(short = "t", long = "initial_time", parse(try_from_str = "chrono::naive::NaiveDateTime::from_str"))]
    pub time_offset: NaiveDateTime,

    /// Number of packet buffers available for received packets, must not be 0
    #[structopt(short = "b", long = "num_buffers", default_value = "100", parse(try_from_str = "crate::config::try_non_zero_usize_from_str"))]
    pub num_buffers: NonZeroUsize,
}

#[derive(Debug, PartialEq)]
pub enum ParseNonZeroIntError {
    Zero,
    IntError(std::num::ParseIntError),
}

impl From<std::num::ParseIntError> for ParseNonZeroIntError {
    fn from(v: std::num::ParseIntError) -> ParseNonZeroIntError {
        ParseNonZeroIntError::IntError(v)
    }
}

impl std::fmt::Display for ParseNonZeroIntError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseNonZeroIntError::Zero => write!(f, "number is zero"),
            ParseNonZeroIntError::IntError(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ParseNonZeroIntError {}

fn try_non_zero_usize_from_str(src: &str) -> std::result::Result<NonZeroUsize, ParseNonZeroIntError> {
    let value = usize::from_str(src)?;
    if 0 == value {
        Err(ParseNonZeroIntError::Zero)
    } else {
        Ok(NonZeroUsize::new(value).unwrap())
    }
}

#[cfg(test)]
mod test {
    use structopt::StructOpt;
    use super::*;

    #[test]
    // try_non_zero_usize_from_str() accepts non zero values
    fn try_non_zero_usize_from_str_valid_value() {
        assert_eq!(Ok(NonZeroUsize::new(42).unwrap()), try_non_zero_usize_from_str(&"42"));
    }

    #[test]
    // try_non_zero_usize_from_str() rejects zero values with ParseNonZeroIntError::Zero
    fn try_non_zero_usize_from_str_zero_value() {
        assert_eq!(Err(ParseNonZeroIntError::Zero), try_non_zero_usize_from_str(&"0"));
    }

    #[test]
    // try_non_zero_usize_from_str() rejects strings not representing numbers
    fn try_non_zero_usize_from_str_no_number() {
        match try_non_zero_usize_from_str(&"a") {
            Err(ParseNonZeroIntError::IntError(_)) => (),
            _ => assert!(false),
        }
    }

    #[test]
    // Correct args in case of sticked options and values
    fn valid_args1() {
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00"]);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        let vsg_addr = Into::<u32>::into(std::net::Ipv4Addr::new(10, 0, 0, 1)).to_be();
        assert_eq!(vsg_addr, config.address);
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
        assert_eq!(100, config.num_buffers.get());
    }

    #[test]
    // Correct args in case of split options and values
    fn valid_args2() {
        let config = Config::from_iter_safe(&["-a", "titi", "-n", "10.0.0.1", "-t", "1970-01-02T00:00:00"]);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        let vsg_addr = Into::<u32>::into(std::net::Ipv4Addr::new(10, 0, 0, 1)).to_be();
        assert_eq!(vsg_addr, config.address);
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
        assert_eq!(100, config.num_buffers.get());
    }

    #[test]
    // Correct args in case of optional buffer pool size
    fn valid_args3() {
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00", "-b1000"]);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        let vsg_addr = Into::<u32>::into(std::net::Ipv4Addr::new(10, 0, 0, 1)).to_be();
        assert_eq!(vsg_addr, config.address);
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
        assert_eq!(1000, config.num_buffers.get());
    }

    #[test]
    // Missing socket value
    fn invalid_args1() {
        assert!(Config::from_iter_safe(&["-a", "-n", "10.0.0.1", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing address value
    fn invalid_args2() {
        assert!(Config::from_iter_safe(&["-a", "titi", "-n", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing time value
    fn invalid_args3() {
        assert!(Config::from_iter_safe(&["-a", "titi", "-n", "10.0.0.1", "-t"]).is_err());
    }

    #[test]
    // Missing time
    fn invalid_args4() {
        assert!(Config::from_iter_safe(&["-a", "titi", "-n", "10.0.0.1"]).is_err());
    }

    #[test]
    // Missing address
    fn invalid_args5() {
        assert!(Config::from_iter_safe(&["-a", "titi", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing actor_socket
    fn invalid_args6() {
        assert!(Config::from_iter_safe(&["-n", "10.0.0.1", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Invalid address format
    fn invalid_args7() {
        assert!(Config::from_iter_safe(&["-atiti", "-n", "10.0.0.1.0", "-t1970-01-02T00:00"]).is_err());
    }

    #[test]
    // Invalid time format
    fn invalid_args8() {
        assert!(Config::from_iter_safe(&["-atiti", "-n", "10.0.0.1", "-t1970-01-02T00:00"]).is_err());
    }

    #[test]
    // Buffer pool size 0 is invalid
    fn invalid_args9() {
        assert!(Config::from_iter_safe(&["-atiti", "-n", "10.0.0.1", "-t1970-01-02T00:00:00", "-b0"]).is_err());
    }
}
