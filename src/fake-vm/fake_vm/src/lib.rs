use buffer_pool::BufferPool;
pub(crate) use config::Config;
use connector::{Connector, ConnectorImpl, MsgIn, MsgOut};
pub use error::Error;
use libc;
#[allow(unused_imports)]
use log::{debug, error};
use output_msg_set::OutputMsgSet;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;
use timer::TimerContext;

pub const MAX_PACKET_SIZE: usize = 2048;

mod buffer_pool;
mod config;
mod connector;
pub mod error;
mod output_msg_set;
mod timer;

impl From<buffer_pool::Error> for Error {
    fn from(error: buffer_pool::Error) -> Error {
        match error {
            buffer_pool::Error::NoBufferAvailable => Error::NoMemoryAvailable,
            buffer_pool::Error::SizeTooBig => Error::SizeTooBig,
        }
    }
}

impl From<output_msg_set::Error> for Error {
    fn from(error: output_msg_set::Error) -> Error {
        match error {
            output_msg_set::Error::NoSlotAvailable {buffer: _} => Error::NoMemoryAvailable,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// Cannot write a generic From<std::io::Result<T>> for Result<T>
fn from_io_result<T>(result: std::io::Result<T>) -> Result<T> {
    result.map_err(Into::into)
}

pub type RecvCallback = Box<Fn(&Context, &[u8]) -> () + Send + Sync>;

// #[derive(Debug)]
// pub struct Destination {
//     addr: u32
// }
pub type VsgAddress = u32;

// InnerContext must be accessed concurrently from application code and the deadline handler. To
// enable this, all fields are either read-only or implement thread and signal handler-safe
// interior mutability.
struct InnerContext {
    // // Time offset added to the global simulation time to get the local simulated time
    // // It is the initial time in the VM, since time in SimGrid starts at 0.
    // // No concurrency, read-only.
    // simulation_offset: NaiveDateTime,
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    connector: Mutex<ConnectorImpl>,
    // No concurrency, read-only: called only by the deadline handler
    recv_callback: RecvCallback,
    // Concurrency:
    // - read-only by application code,
    // - read-write by the deadline handler, using interior mutability
    timer_context: TimerContext,
    // Concurrency: Buffers are:
    // - allocated and added to the set by application code,
    // - consumed and freed by the deadline handler.
    // BufferPool uses interior mutability for concurrent allocation and freeing of buffers.
    output_buffer_pool: BufferPool,
    outgoing_messages: OutputMsgSet,
    // Concurrency: none
    // Prevents application from starting twice
    start_once: Once,
}

impl std::fmt::Debug for InnerContext {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "InnerContext {{ connector: {:?}, timer_context: {:?}, output_buffer_pool: {:?}, outgoing_messages: {:?}, start_once: {:?} }}", self.connector, self.timer_context, self.output_buffer_pool, self.outgoing_messages, self.start_once)
    }
}

impl InnerContext {
    fn new(config: &Config, recv_callback: RecvCallback) -> Result<InnerContext> {
        let connector = ConnectorImpl::new(&config)?;
        let timer_context = TimerContext::new(&config)?;
        let output_buffer_pool = BufferPool::new(crate::MAX_PACKET_SIZE, config.num_buffers.get());
        let outgoing_messages = OutputMsgSet::new(config.num_buffers.get());

        Ok(InnerContext {
            // simulation_offset: config.time_offset,
            connector: Mutex::new(connector),
            recv_callback: recv_callback,
            timer_context: timer_context,
            output_buffer_pool: output_buffer_pool,
            outgoing_messages: outgoing_messages,
            start_once: Once::new(),
        })
    }

    fn start(&self, deadline: Duration) -> Result<()> {
        from_io_result(self.timer_context.start(deadline))
    }

    fn gettimeofday(&self) -> libc::timeval {
        let adjusted_time = self.timer_context.application_now();
        libc::timeval {
            tv_sec:  adjusted_time.timestamp() as libc::time_t,
            tv_usec: adjusted_time.timestamp_subsec_micros() as libc::suseconds_t,
        }
    }

    fn send(&self, src: VsgAddress, dest: VsgAddress, msg: &[u8]) -> Result<()> {
        let mut buffer = self.output_buffer_pool.allocate_buffer(msg.len())?;
        buffer.copy_from_slice(msg);

        let send_time = self.timer_context.simulation_now();
        // It is possible that the deadline is reached just after recording the send time and
        // before inserting the message, which leads to sending the message at the next deadline.
        // This would violate the property that send times must be after the previous deadline
        // (included) and (strictly) before the current deadline. To solve this, ::at_deadline()
        // takes the latest time between the recorded time and the previous deadline.
        self.outgoing_messages.insert(send_time, src, dest, buffer)?;
        Ok(())
    }
}

#[derive(Debug)]
enum AfterDeadline {
    NextDeadline(Duration),
    EndSimulation,
}

#[derive(Debug)]
pub struct Context(Arc<InnerContext>);

impl Context {
    fn new(config: &Config, recv_callback: RecvCallback) -> Result<Box<Context>> {
        let inner_context = InnerContext::new(config, recv_callback)?;
        let context = Box::new(Context(Arc::new(inner_context)));
        timer::register(&context)?;

        Ok(context)
    }

    pub fn start(&self) -> Result<()> {
        let context = &self.0;
        let mut res = Err(Error::AlreadyStarted);

        context.start_once.call_once(|| res = (|| {
            let mut connector = context.connector.lock().unwrap();
            let msg = connector.recv()?;
            // The deadline handler can fire and try to lock connector at any time once self.0.start()
            // is called so we must unlock connector before.
            drop(connector);
            match msg {
                MsgIn::GoToDeadline(deadline) => context.start(deadline),
                _ => Err(Error::ProtocolViolation),
            }
        })());

        res
    }

    pub fn stop(&self) {
        self.0.timer_context.stop()
    }

    fn at_deadline(&self) -> AfterDeadline {
        let mut connector = self.0.connector.lock().unwrap();

        // First, send all messages from this last time slice to others
        let messages = self.0.outgoing_messages.drain();
        let previous_deadline = self.0.timer_context.simulation_previous_deadline();
        let current_deadline = self.0.timer_context.simulation_next_deadline();
        for (send_time, src, dest, payload) in messages {
            let send_time = if send_time < previous_deadline {
                // This message was time-stamped before the previous deadline but inserted after.
                // Fix the timestamp to stay between the deadlines.
                previous_deadline
            } else {
                if send_time >= current_deadline {
                    // The kernel was too slow to fire the timer...
                    return AfterDeadline::EndSimulation;
                }
                send_time
            };

            if let Err(e) = connector.send(MsgOut::SendPacket(send_time, src, dest, payload)) {
                // error!("send(SendPacket) failed: {}", e);
                return AfterDeadline::EndSimulation;
            }
        }

        // Second, notify that we reached the deadline
        if let Err(e) = connector.send(MsgOut::AtDeadline) {
            // error!("send(AtDeadline) failed: {}", e);
            return AfterDeadline::EndSimulation;
        }

        // Third, receive messages from others, followed by next deadline
        loop {
            let msg = connector.recv();
            match msg {
                Ok(msg) => if let Some(after_deadline) = self.handle_actor_msg(msg) {
                    return after_deadline;
                },
                Err(e) => {
                    // error!("recv failed: {}", e);
                    return AfterDeadline::EndSimulation;
                }
            }
        };
    }

    fn handle_actor_msg(&self, msg: MsgIn) -> Option<AfterDeadline> {
        match msg {
            MsgIn::DeliverPacket(packet) => {
                (self.0.recv_callback)(self, &packet);
                None
            },
            MsgIn::GoToDeadline(deadline) => Some(AfterDeadline::NextDeadline(deadline)),
            MsgIn::EndSimulation => Some(AfterDeadline::EndSimulation),
        }
    }

    pub fn gettimeofday(&self) -> libc::timeval {
        self.0.gettimeofday()
    }

    pub fn send(&self, src: VsgAddress, dest: VsgAddress, msg: &[u8]) -> Result<()> {
        self.0.send(src, dest, msg)
    }

    pub fn poll(&self, buffer: &mut [u8]) -> Result<()> {
        unimplemented!()
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

#[cfg(any(test, feature = "test-helpers"))]
#[macro_use]
pub mod test_helpers {
    use std::time::Duration;
    use super::connector::{MsgIn, MsgOut};
    #[cfg(feature = "test-helpers")]
    pub use super::connector::test_helpers::*;

    #[macro_export]
    macro_rules! valid_args {
        () => {
            &["-atiti", "-t1970-01-01T00:00:00"]
        }
    }

    #[macro_export]
    macro_rules! valid_args_h1 {
        () => {
            &["-atiti", "-t1970-01-01T01:00:00"]
        }
    }

    #[macro_export]
    macro_rules! invalid_args {
        () => {
            &["-btiti", "-t1970-01-01T00:00:00"]
        }
    }

    pub fn dummy_recv_callback(_context: &super::Context, _packet: &[u8]) -> () {
    }

    pub fn start_actor(actor: &mut TestActor) -> TestResult<()> {
        actor.send(MsgIn::GoToDeadline(Duration::new(0, 100000)))?;
        actor.send(MsgIn::EndSimulation)
    }

    // Actor that will let the VM run until the VM explicitly stops, by either sending a packet
    // (clean stop) or just closing the connection (reported as an error without making the test
    // fail)
    pub fn recv_one_msg_actor(actor: &mut TestActor) -> TestResult<()> {
        loop {
            actor.send(MsgIn::GoToDeadline(Duration::new(0, 100000)))?;
            let msg = actor.recv()?;
            match msg {
                MsgOut::AtDeadline => (),
                MsgOut::SendPacket(_, _, _, _) => break,
            }
        }
        actor.send(MsgIn::EndSimulation)
    }


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
    use super::connector::Connector;
    use super::{connector::test_helpers::*, test_helpers::*};

    #[test]
    fn init_valid() {
        init();

        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        super::init(valid_args!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        // assert_eq!(chrono::NaiveDateTime::from_timestamp(0, 0), context.0.simulation_offset);

        drop(actor);
    }

    #[test]
    fn init_invalid() {
        init();

        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        super::init(invalid_args!(), Box::new(dummy_recv_callback))
            .expect_err("init returned a context");

        drop(actor);
    }

    #[test]
    fn start_stop() {
        init();

        let actor = TestActorDesc::new("titi", start_actor);
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        context.start()
            .expect("start failed");

        context.stop();

        drop(actor);
    }

    #[test]
    fn start_already() {
        init();

        let actor = TestActorDesc::new("titi", start_actor);
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        context.start()
            .expect("start failed");

        match context.start().expect_err("start should have failed") {
            super::error::Error::AlreadyStarted => (),
            _ => assert!(false),
        }

        context.stop();

        drop(actor);
    }

    #[test]
    fn send() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        context.start()
            .expect("start failed");

        let src = 0;
        let dest = 1;
        context.send(src, dest, b"Foo msg")
            .expect("send failed");

        context.stop();

        drop(actor);
    }

    #[test]
    fn gettimeofday() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        context.start()
            .expect("start failed");

        let tv = context.gettimeofday();
        // 10 seconds should be enough for slow machines...
        assert!(tv.tv_sec >= 0 && tv.tv_sec < 10);
        assert!(tv.tv_usec >= 0 && tv.tv_usec < 999999);

        let src = 0;
        let dest = 1;
        context.send(src, dest, b"This is the end")
            .expect("send failed");

        context.stop();

        drop(actor);
    }

    #[test]
    fn gettimeofday_h1() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let context = super::init(valid_args_h1!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        context.start()
            .expect("start failed");

        let tv = context.gettimeofday();
        // 10 seconds should be enough for slow machines...
        assert!(tv.tv_sec >= 3600 && tv.tv_sec < 3610);
        assert!(tv.tv_usec >= 0 && tv.tv_usec < 999999);

        let src = 0;
        let dest = 1;
        context.send(src, dest, b"This is the end")
            .expect("send failed");

        context.stop();

        drop(actor);
    }

    #[test]
    fn message_loop() {
        init();

        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let context = super::init(valid_args_h1!(), Box::new(dummy_recv_callback))
            .expect("init() failed");

        let mut connector = context.0.connector.lock().unwrap();

        loop {
            let msg = connector.recv();
            match msg {
                Ok(msg) => if let Some(after_deadline) = context.handle_actor_msg(msg) {
                    info!("after_deadline is: {:?}", after_deadline);
                },
                Err(e) => match e.kind() {
                    std::io::ErrorKind::Interrupted => info!("recv interrupted"),
                    _ => {
                        error!("recv failed: {}", e);
                        break;
                    },
                },
            }
        }

        drop(actor);
    }
}
