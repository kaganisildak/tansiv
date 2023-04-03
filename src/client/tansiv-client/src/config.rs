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

    /// Bounded packets queue size to simulate, must not be 0
    #[structopt(short = "q", long = "queue_size", default_value = "1024")]
    pub queue_size: NonZeroUsize,

    /// Uplink bandwidth in bits per second, must not be 0
    #[structopt(short = "w", long = "uplink_bandwidth")]
    pub uplink_bandwidth: NonZeroUsize,

    /// Uplink overhead in bytes per packet (preample, inter-frame gap...)
    #[structopt(short = "x", long = "uplink_overhead")]
    pub uplink_overhead: usize,

    /// Initial time in the VM, formatted as %Y-%m-%dT%H:%M:%S%.f (%.f part is optional)
    #[structopt(short = "t", long = "initial_time", parse(try_from_str = "chrono::naive::NaiveDateTime::from_str"))]
    pub time_offset: NaiveDateTime,

    /// Number of packet buffers available for received packets, must not be 0
    #[structopt(short = "b", long = "num_buffers", default_value = "100")]
    pub num_buffers: NonZeroUsize,
}

#[cfg(test)]
mod test {
    use structopt::StructOpt;
    use super::*;

    #[test]
    // Correct args in case of sticked options and values
    fn valid_args1() {
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-w100000000", "-x24", "-t1970-01-02T00:00:00"]);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        let vsg_addr = Into::<u32>::into(std::net::Ipv4Addr::new(10, 0, 0, 1)).to_be();
        assert_eq!(vsg_addr, config.address);
        assert_eq!(100_000_000, config.uplink_bandwidth.get());
        assert_eq!(24, config.uplink_overhead);
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
        assert_eq!(100, config.num_buffers.get());
    }

    #[test]
    // Correct args in case of split options and values
    fn valid_args2() {
        let config = Config::from_iter_safe(&["-a", "titi", "-n", "10.0.0.1", "-w", "100000000", "-x", "24", "-t", "1970-01-02T00:00:00"]);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        let vsg_addr = Into::<u32>::into(std::net::Ipv4Addr::new(10, 0, 0, 1)).to_be();
        assert_eq!(vsg_addr, config.address);
        assert_eq!(100_000_000, config.uplink_bandwidth.get());
        assert_eq!(24, config.uplink_overhead);
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
        assert_eq!(100, config.num_buffers.get());
    }

    #[test]
    // Correct args in case of optional buffer pool size
    fn valid_args3() {
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-w100000000", "-x24", "-t1970-01-02T00:00:00", "-b1000"]);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        let vsg_addr = Into::<u32>::into(std::net::Ipv4Addr::new(10, 0, 0, 1)).to_be();
        assert_eq!(vsg_addr, config.address);
        assert_eq!(100_000_000, config.uplink_bandwidth.get());
        assert_eq!(24, config.uplink_overhead);
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
        assert_eq!(1000, config.num_buffers.get());
    }

    #[test]
    // Correct args in case of optional queue size
    fn valid_args4() {
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-w100000000", "-x24", "-t1970-01-02T00:00:00", "-q2048"]);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!("titi", config.actor_socket.to_str().unwrap());
        let vsg_addr = Into::<u32>::into(std::net::Ipv4Addr::new(10, 0, 0, 1)).to_be();
        assert_eq!(vsg_addr, config.address);
        assert_eq!(100_000_000, config.uplink_bandwidth.get());
        assert_eq!(24, config.uplink_overhead);
        assert_eq!(NaiveDateTime::from_timestamp(86400, 0), config.time_offset);
        assert_eq!(2048, config.queue_size.get());
    }

    #[test]
    // Missing socket value
    fn invalid_args1() {
        assert!(Config::from_iter_safe(&["-a", "-n", "10.0.0.1", "-w100000000", "-x24", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing address value
    fn invalid_args2() {
        assert!(Config::from_iter_safe(&["-a", "titi", "-n", "-w100000000", "-x24", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing time value
    fn invalid_args3() {
        assert!(Config::from_iter_safe(&["-a", "titi", "-n", "10.0.0.1", "-w100000000", "-x24", "-t"]).is_err());
    }

    #[test]
    // Missing time
    fn invalid_args4() {
        assert!(Config::from_iter_safe(&["-a", "titi", "-n", "10.0.0.1", "-w100000000", "-x24"]).is_err());
    }

    #[test]
    // Missing address
    fn invalid_args5() {
        assert!(Config::from_iter_safe(&["-a", "titi", "-w100000000", "-x24", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing actor_socket
    fn invalid_args6() {
        assert!(Config::from_iter_safe(&["-n", "10.0.0.1", "-w100000000", "-x24", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Invalid address format
    fn invalid_args7() {
        assert!(Config::from_iter_safe(&["-atiti", "-n", "10.0.0.1.0", "-w100000000", "-x24", "-t1970-01-02T00:00"]).is_err());
    }

    #[test]
    // Invalid time format
    fn invalid_args8() {
        assert!(Config::from_iter_safe(&["-atiti", "-n", "10.0.0.1", "-w100000000", "-x24", "-t1970-01-02T00:00"]).is_err());
    }

    #[test]
    // Buffer pool size 0 is invalid
    fn invalid_args9() {
        assert!(Config::from_iter_safe(&["-atiti", "-n", "10.0.0.1", "-w100000000", "-x24", "-t1970-01-02T00:00:00", "-b0"]).is_err());
    }

    #[test]
    // Missing queue size value
    fn invalid_args10() {
        assert!(Config::from_iter_safe(&["-a", "-n", "10.0.0.1", "-q", "-w100000000", "-x24", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing uplink bandwidth value
    fn invalid_args11() {
        assert!(Config::from_iter_safe(&["-a", "-n", "10.0.0.1", "-w", "-x24", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing uplink overhead value
    fn invalid_args12() {
        assert!(Config::from_iter_safe(&["-a", "-n", "10.0.0.1", "-w100000000", "-x", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing uplink bandwidth
    fn invalid_args13() {
        assert!(Config::from_iter_safe(&["-a", "-n", "10.0.0.1", "-x24", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Missing uplink overhead
    fn invalid_args14() {
        assert!(Config::from_iter_safe(&["-a", "-n", "10.0.0.1", "-w100000000", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Queue size 0 is invalid
    fn invalid_args15() {
        assert!(Config::from_iter_safe(&["-a", "-n", "10.0.0.1", "-q0", "-w0", "-x24", "-t1970-01-02T00:00:00"]).is_err());
    }

    #[test]
    // Uplink bandwidth 0 is invalid
    fn invalid_args16() {
        assert!(Config::from_iter_safe(&["-a", "-n", "10.0.0.1", "-w0", "-x24", "-t1970-01-02T00:00:00"]).is_err());
    }
}
