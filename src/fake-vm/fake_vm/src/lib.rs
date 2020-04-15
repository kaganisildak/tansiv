use buffer_pool::{Buffer, BufferPool};
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
use waitfree_array_queue::WaitfreeArrayQueue;

pub const MAX_PACKET_SIZE: usize = 2048;

mod buffer_pool;
mod config;
mod connector;
pub mod error;
mod output_msg_set;
mod timer;
mod waitfree_array_queue;

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

pub type RecvCallback = Box<Fn(&Context, &[u8]) -> () + Send + Sync>;

// #[derive(Debug)]
// pub struct Destination {
//     addr: u32
// }
pub type VsgAddress = u32;

fn vsg_address_from_str(ip: &str) -> std::result::Result<VsgAddress, std::net::AddrParseError> {
    use std::str::FromStr;
    let ipv4 = std::net::Ipv4Addr::from_str(ip)?;
    Ok(Into::<u32>::into(ipv4).to_be())
}

#[derive(Debug)]
struct Packet {
    src: VsgAddress,
    dst: VsgAddress,
    payload: Buffer,
}

impl Packet {
    fn new(src: VsgAddress, dst: VsgAddress, payload: Buffer) -> Packet {
        Packet {
            src: src,
            dst: dst,
            payload: payload,
        }
    }
}

// InnerContext must be accessed concurrently from application code and the deadline handler. To
// enable this, all fields are either read-only or implement thread and signal handler-safe
// interior mutability.
struct InnerContext {
    // Read-only
    address: VsgAddress,
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    connector: Mutex<ConnectorImpl>,
    // Concurrency: Messages are:
    // - pushed to the queue by the deadline handler,
    // - popped from the queue by application code.
    // Concurrent read-write support is provided by interior mutability.
    input_queue: WaitfreeArrayQueue<Packet>,
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
        write!(f, "InnerContext {{ address: {:0x}, connector: {:?}, input_queue: {:?}, timer_context: {:?}, output_buffer_pool: {:?}, outgoing_messages: {:?}, start_once: {:?} }}", self.address, self.connector, self.input_queue, self.timer_context, self.output_buffer_pool, self.outgoing_messages, self.start_once)
    }
}

impl InnerContext {
    fn new(config: &Config, recv_callback: RecvCallback) -> Result<InnerContext> {
        let address = config.address;
        let connector = ConnectorImpl::new(&config)?;
        let input_queue = WaitfreeArrayQueue::new(config.num_buffers.get());
        let timer_context = TimerContext::new(&config)?;
        let output_buffer_pool = BufferPool::new(crate::MAX_PACKET_SIZE, config.num_buffers.get());
        let outgoing_messages = OutputMsgSet::new(config.num_buffers.get());

        Ok(InnerContext {
            address: address,
            connector: Mutex::new(connector),
            input_queue: input_queue,
            recv_callback: recv_callback,
            timer_context: timer_context,
            output_buffer_pool: output_buffer_pool,
            outgoing_messages: outgoing_messages,
            start_once: Once::new(),
        })
    }

    fn start(&self, deadline: Duration) -> Result<()> {
        Ok(self.timer_context.start(deadline)?)
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
        let input_queue = &self.0.input_queue;
        // let may_notify = input_queue.is_empty();

        let after_deadline = loop {
            let msg = connector.recv();
            match msg {
                Ok(msg) => if let Some(after_deadline) = self.handle_actor_msg(msg) {
                    break after_deadline;
                },
                Err(e) => {
                    // error!("recv failed: {}", e);
                    break AfterDeadline::EndSimulation;
                }
            }
        };

        // if may_notify && !input_queue.is_empty() {
        // (self.0.recv_callback)(self.0.recv_token);
        // }
        for packet in input_queue.iter() {
            (self.0.recv_callback)(self, &packet.payload);
        }

        after_deadline
    }

    fn handle_actor_msg(&self, msg: MsgIn) -> Option<AfterDeadline> {
        match msg {
            MsgIn::DeliverPacket(packet) => {
                // FIXME: Use src address when available in the protocol
                let src = self.0.address;
                // Or use dst address when available in the protocol? Should be the same...
                let dst = self.0.address;
                let size = packet.len();
                if self.0.input_queue.push(Packet::new(src, dst, packet)).is_err() {
                    // info!("Dropping input packet from {} of {} bytes", src, size);
                }
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
    macro_rules! local_vsg_address_str {
        () => {
            "10.0.0.1"
        }
    }

    #[macro_export]
    macro_rules! local_vsg_address {
        () => {{
            use std::str::FromStr;

            u32::from(std::net::Ipv4Addr::from_str(local_vsg_address_str!()).unwrap()).to_be()
        }}
    }

    #[macro_export]
    macro_rules! remote_vsg_address {
        () => {
            u32::from(std::net::Ipv4Addr::new(10, 0, 1, 1)).to_be()
        }
    }

    #[macro_export]
    macro_rules! valid_args {
        () => {
            &["-atiti", "-n", local_vsg_address_str!(), "-t1970-01-01T00:00:00"]
        }
    }

    #[macro_export]
    macro_rules! valid_args_h1 {
        () => {
            &["-atiti", "-n", local_vsg_address_str!(), "-t1970-01-01T01:00:00"]
        }
    }

    #[macro_export]
    macro_rules! invalid_args {
        () => {
            &["-btiti", "-n", local_vsg_address_str!(), "-t1970-01-01T00:00:00"]
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

        let src = local_vsg_address!();
        let dest = remote_vsg_address!();
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

        let src = local_vsg_address!();
        let dest = remote_vsg_address!();
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

        let src = local_vsg_address!();
        let dest = remote_vsg_address!();
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
