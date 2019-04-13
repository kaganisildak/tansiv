use chrono::naive::NaiveDateTime;
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

#[cfg(test)]
mod test {
    use structopt::StructOpt;
    use super::*;

    #[test]
    // Correct args in case of sticked options and values
    fn valid_args1() {
        let config = Config::from_iter_safe(&["-atiti", "-t1970-01-02T00:00:00"]);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
    }

    #[test]
    // Correct args in case of split options and values
    fn valid_args2() {
        let config = Config::from_iter_safe(&["-a", "titi", "-t", "1970-01-02T00:00:00"]);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
    }

    #[test]
    // Missing socket value
    fn invalid_args1() {
        assert!(Config::from_iter_safe(&["-a", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing time value
    fn invalid_args2() {
        assert!(Config::from_iter_safe(&["-a", "titi", "-t"]).is_err());
    }

    #[test]
    // Missing time
    fn invalid_args3() {
        assert!(Config::from_iter_safe(&["-a", "titi"]).is_err());
    }

    #[test]
    // Missing actor_socket
    fn invalid_args4() {
        assert!(Config::from_iter_safe(&["-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Invalid time format
    fn invalid_args5() {
        assert!(Config::from_iter_safe(&["-atiti", "-t1970-01-02T00:00"]).is_err());
    }
}
