// Use chrono::Duration (re-exported from time::Duration) to allow negative values, which are not
// allowed in std::time::Duration
use chrono::{Duration, naive::NaiveDateTime};
use buffer_pool::BufferPool;
pub(crate) use config::Config;
use connector::{Connector, ConnectorImpl, MsgIn, MsgOut};
pub use error::Error;
use libc;
#[allow(unused_imports)]
use log::{debug, error};

pub const MAX_PACKET_SIZE: usize = 2048;

mod buffer_pool;
mod config;
mod connector;
pub mod error;

impl From<buffer_pool::Error> for Error {
    fn from(error: buffer_pool::Error) -> Error {
        match error {
            buffer_pool::Error::NoBufferAvailable => Error::NoMemoryAvailable,
            buffer_pool::Error::SizeTooBig => Error::SizeTooBig,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// Cannot write a generic From<std::io::Result<T>> for Result<T>
fn from_io_result<T>(result: std::io::Result<T>) -> Result<T> {
    result.map_err(Into::into)
}

pub type RecvCallback = Box<FnMut(&Context, &[u8]) -> ()>;

pub struct Context {
    time_offset: Duration,
    connector: ConnectorImpl,
    input_buffer_pool: BufferPool,
    recv_callback: RecvCallback,
}

impl Context {
    fn new(config: &Config, recv_callback: RecvCallback) -> Result<Box<Context>> {
        // Here is where the time reference is recorded
        let time_offset = config.time_offset - chrono::offset::Local::now().naive_local();
        let (connector, input_buffer_pool) = ConnectorImpl::new(&config)?;

        Ok(Box::new(Context {
                time_offset: time_offset,
                connector: connector,
                input_buffer_pool: input_buffer_pool,
                recv_callback: recv_callback,
        }))
    }
}

pub fn init<I>(args: I, recv_callback: RecvCallback) -> Result<Box<Context>>
    where I: IntoIterator,
          I::Item: Into<std::ffi::OsString> + Clone {
    use structopt::StructOpt;

    let config = Config::from_iter_safe(args).or_else(|e| Err(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    debug!("{:?}", config);

    Context::new(&config, recv_callback)
}

pub fn gettimeofday(context: &Context) -> libc::timeval {
    let adjusted_time = chrono::offset::Local::now().naive_local() + context.time_offset;
    libc::timeval {
        tv_sec:  adjusted_time.timestamp() as libc::time_t,
        tv_usec: adjusted_time.timestamp_subsec_micros() as libc::suseconds_t,
    }
}

fn handle_actor_msg(connector: &mut ConnectorImpl, msg: MsgIn) {
    match msg {
        MsgIn::DeliverPacket(packet) => {
        }
        MsgIn::GoToDeadline(deadline) => {
        }
    }
}

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    #[cfg(feature = "test-helpers")]
    pub use super::connector::test_helpers::*;

    static INIT: std::sync::Once = std::sync::ONCE_INIT;

    pub fn init() {
        // Cargo test runs all tests in a same process so don't confuse log by setting a logger
        // several times.
        INIT.call_once(|| stderrlog::new().module(module_path!()).verbosity(log::Level::Info as usize).init().unwrap());
    }
}

#[cfg(test)]
mod test {
    #[allow(unused_imports)]
    use log::{error, info};
    use std::path::PathBuf;
    use super::connector::Connector;
    use super::{connector::test_helpers::*, test_helpers::*};

    macro_rules! valid_args {
        () => {
            &["-atiti", "-t1970-01-01T00:00:00"]
        }
    }
    macro_rules! valid_args_h1 {
        () => {
            &["-atiti", "-t1970-01-01T01:00:00"]
        }
    }
    macro_rules! invalid_args {
        () => {
            &["-btiti", "-t1970-01-01T00:00:00"]
        }
    }

    fn dummy_recv_callback(_context: &super::Context, _packet: &[u8]) -> () {
    }

    #[test]
    fn init_valid() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        let context = context.unwrap();
        info!("context.time_offset: {:?}", context.time_offset);

        test_cleanup_connect(&server_path);
    }

    #[test]
    fn init_invalid() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let context = super::init(invalid_args!(), Box::new(dummy_recv_callback))
            .expect_err("init returned a context");

        test_cleanup_connect(&server_path);
    }

    #[test]
    fn gettimeofday() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let mut context = super::init(valid_args!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        let tv = super::gettimeofday(&mut context);
        // 10 seconds should be enough for slow machines...
        assert!(tv.tv_sec >= 0 && tv.tv_sec < 10);
        assert!(tv.tv_usec >= 0 && tv.tv_usec < 999999);

        test_cleanup_connect(&server_path);
    }

    #[test]
    fn gettimeofday_h1() {
        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let mut context = super::init(valid_args_h1!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        let tv = super::gettimeofday(&mut context);
        // 10 seconds should be enough for slow machines...
        assert!(tv.tv_sec >= 3600 && tv.tv_sec < 3610);
        assert!(tv.tv_usec >= 0 && tv.tv_usec < 999999);

        test_cleanup_connect(&server_path);
    }

    #[test]
    fn message_loop() {
        use std::ops::DerefMut;

        init();

        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let context = super::init(valid_args_h1!(), Box::new(dummy_recv_callback))
            .expect("init() failed");

        let mut context = context.unwrap();
        let connector = &mut context.connector;
        let input_buffer_pool = &context.input_buffer_pool;
        let mut allocated_input_buffer = input_buffer_pool.allocate_buffer(super::MAX_PACKET_SIZE).unwrap();
        let input_buffer = allocated_input_buffer.deref_mut();

        loop {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(msg) => super::handle_actor_msg(connector, msg),
                Err(e) => match e.kind() {
                    std::io::ErrorKind::Interrupted => info!("recv interrupted"),
                    _ => {
                        error!("recv failed: {}", e);
                        break;
                    },
                },
            }
        }

        test_cleanup_connect(&server_path);
    }
}
