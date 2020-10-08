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

pub type RecvCallback = Box<dyn Fn() -> () + Send + Sync>;

fn vsg_address_from_str(ip: &str) -> std::result::Result<libc::in_addr_t , std::net::AddrParseError> {
    use std::str::FromStr;
    let ipv4 = std::net::Ipv4Addr::from_str(ip)?;
    Ok(Into::<u32>::into(ipv4).to_be())
}

#[derive(Debug)]
struct Packet {
    src: libc::in_addr_t,
    dst: libc::in_addr_t,
    payload: Buffer,
}

impl Packet {
    fn new(src: libc::in_addr_t, dst: libc::in_addr_t, payload: Buffer) -> Packet {
        Packet {
            src: src,
            dst: dst,
            payload: payload,
        }
    }
}

// Context must be accessed concurrently from application code and the deadline handler. To
// enable this, all fields are either read-only or implement thread and signal handler-safe
// interior mutability.
pub struct Context {
    // Read-only
    address: libc::in_addr_t,
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

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "Context {{ address: {:0x}, connector: {:?}, input_queue: {:?}, timer_context: {:?}, output_buffer_pool: {:?}, outgoing_messages: {:?}, start_once: {:?} }}", self.address, self.connector, self.input_queue, self.timer_context, self.output_buffer_pool, self.outgoing_messages, self.start_once)
    }
}

#[derive(Debug)]
enum AfterDeadline {
    NextDeadline(Duration),
    EndSimulation,
}

impl Context {
    fn new(config: &Config, recv_callback: RecvCallback) -> Result<Arc<Context>> {
        let address = config.address;
        let connector = ConnectorImpl::new(config)?;
        let input_queue = WaitfreeArrayQueue::new(config.num_buffers.get());
        let timer_context = TimerContext::new(config)?;
        let output_buffer_pool = BufferPool::new(crate::MAX_PACKET_SIZE, config.num_buffers.get());
        let outgoing_messages = OutputMsgSet::new(config.num_buffers.get());

        let context = Arc::new(Context {
            address: address,
            connector: Mutex::new(connector),
            input_queue: input_queue,
            recv_callback: recv_callback,
            timer_context: timer_context,
            output_buffer_pool: output_buffer_pool,
            outgoing_messages: outgoing_messages,
            start_once: Once::new(),
        });
        timer::register(&context)?;

        Ok(context)
    }

    pub fn start(&self) -> Result<()> {
        let mut res = Err(Error::AlreadyStarted);

        self.start_once.call_once(|| res = (|| {
            let mut connector = self.connector.lock().unwrap();
            let msg = connector.recv()?;
            // The deadline handler can fire and try to lock connector at any time once self.0.start()
            // is called so we must unlock connector before.
            drop(connector);
            match msg {
                // Writing Ok(...?) helps the compiler to know how to convert std::io::Error to Error
                MsgIn::GoToDeadline(deadline) => Ok(self.timer_context.start(deadline)?),
                _ => Err(Error::ProtocolViolation),
            }
        })());

        res
    }

    pub fn stop(&self) {
        self.timer_context.stop()
    }

    fn at_deadline(&self) -> AfterDeadline {
        let mut connector = self.connector.lock().unwrap();

        // First, send all messages from this last time slice to others
        let messages = self.outgoing_messages.drain();
        let previous_deadline = self.timer_context.simulation_previous_deadline();
        let current_deadline = self.timer_context.simulation_next_deadline();
        for (send_time, src, dst, payload) in messages {
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

            if let Err(_e) = connector.send(MsgOut::SendPacket(send_time, src, dst, payload)) {
                // error!("send(SendPacket) failed: {}", _e);
                return AfterDeadline::EndSimulation;
            }
        }

        // Second, notify that we reached the deadline
        if let Err(_e) = connector.send(MsgOut::AtDeadline) {
            // error!("send(AtDeadline) failed: {}", _e);
            return AfterDeadline::EndSimulation;
        }

        // Third, receive messages from others, followed by next deadline
        let input_queue = &self.input_queue;
        let may_notify = input_queue.is_empty();

        let after_deadline = loop {
            let msg = connector.recv();
            match msg {
                Ok(msg) => if let Some(after_deadline) = self.handle_actor_msg(msg) {
                    break after_deadline;
                },
                Err(_e) => {
                    // error!("recv failed: {}", _e);
                    break AfterDeadline::EndSimulation;
                }
            }
        };

        if may_notify && !input_queue.is_empty() {
            (self.recv_callback)();
        }

        after_deadline
    }

    fn handle_actor_msg(&self, msg: MsgIn) -> Option<AfterDeadline> {
        match msg {
            MsgIn::DeliverPacket(src, dst, packet) => {
                // let size = packet.len();
                if self.input_queue.push(Packet::new(src, dst, packet)).is_err() {
                    // info!("Dropping input packet from {} of {} bytes", src, size);
                }
                None
            },
            MsgIn::GoToDeadline(deadline) => Some(AfterDeadline::NextDeadline(deadline)),
            MsgIn::EndSimulation => Some(AfterDeadline::EndSimulation),
        }
    }

    pub fn gettimeofday(&self) -> libc::timeval {
        let adjusted_time = self.timer_context.application_now();
        libc::timeval {
            tv_sec:  adjusted_time.timestamp() as libc::time_t,
            tv_usec: adjusted_time.timestamp_subsec_micros() as libc::suseconds_t,
        }
    }

    pub fn send(&self, dst: libc::in_addr_t, msg: &[u8]) -> Result<()> {
        let mut buffer = self.output_buffer_pool.allocate_buffer(msg.len())?;
        buffer.copy_from_slice(msg);

        let send_time = self.timer_context.simulation_now();
        // It is possible that the deadline is reached just after recording the send time and
        // before inserting the message, which leads to sending the message at the next deadline.
        // This would violate the property that send times must be after the previous deadline
        // (included) and (strictly) before the current deadline. To solve this, ::at_deadline()
        // takes the latest time between the recorded time and the previous deadline.
        self.outgoing_messages.insert(send_time, self.address,  dst, buffer)?;
        Ok(())
    }

    pub fn recv<'a, 'b>(&'a self, msg: &'b mut [u8]) -> Result<(libc::in_addr_t, libc::in_addr_t, &'b mut [u8])> {
        match self.input_queue.pop() {
            Some(Packet { src, dst, payload, }) => {
                if msg.len() >= payload.len() {
                    let msg = &mut msg[..payload.len()];
                    msg.copy_from_slice(&payload);
                    Ok((src, dst, msg))
                } else {
                    Err(Error::SizeTooBig)
                }
            },
            None => Err(Error::NoMessageAvailable),
        }
    }

    pub fn poll(&self) -> Option<()> {
        if self.input_queue.is_empty() {
            None
        } else {
            Some(())
        }
    }
}

pub fn init<I>(args: I, recv_callback: RecvCallback) -> Result<Arc<Context>>
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
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

    pub fn dummy_recv_callback() -> () {
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

    pub fn send_one_msg_actor(actor: &mut TestActor, msg: &[u8]) -> TestResult<()> {
        send_one_delayed_msg_actor(actor, msg, 100, 100)
    }

    pub fn send_one_delayed_msg_actor(actor: &mut TestActor, msg: &[u8], slice_micros: u64, delay_micros: u64) -> TestResult<()> {
        let buffer_pool = crate::BufferPool::new(msg.len(), 1);
        let mut buffer = TestActor::check(buffer_pool.allocate_buffer(msg.len()), "Buffer allocation failed")?;
        (&mut buffer).copy_from_slice(msg);

        let mut next_deadline_micros = slice_micros;
        while next_deadline_micros < delay_micros {
            actor.send(MsgIn::GoToDeadline(Duration::from_micros(slice_micros)))?;
            loop {
                match actor.recv()? {
                    MsgOut::AtDeadline => break,
                    _ => (),
                }
            }

            next_deadline_micros += slice_micros;
        }

        let next_slice = delay_micros - (next_deadline_micros - slice_micros);
        actor.send(MsgIn::GoToDeadline(Duration::from_micros(next_slice)))?;
        let src = local_vsg_address!();
        let dst = remote_vsg_address!();
        actor.send(MsgIn::DeliverPacket(src, dst, buffer))?;
        actor.send(MsgIn::EndSimulation)
    }

    #[derive(Clone)]
    pub struct RecvNotifier(Arc<AtomicBool>);

    impl RecvNotifier {
        pub fn new() -> RecvNotifier {
            RecvNotifier(Arc::new(AtomicBool::new(false)))
        }

        pub fn notify(&self) -> () {
            self.0.store(true, Ordering::SeqCst);
        }

        pub fn wait(&self, pause_slice_micros: u64) -> () {
            while !self.0.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_micros(pause_slice_micros));
            }
            self.0.store(false, Ordering::SeqCst);
        }

        pub fn get_callback(&self) -> crate::RecvCallback {
            let cb_notifier = self.clone();
            Box::new(move || cb_notifier.notify())
        }
    }

    static INIT: std::sync::Once = std::sync::Once::new();

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

        let dst = remote_vsg_address!();
        context.send(dst, b"Foo msg")
            .expect("send failed");

        context.stop();

        drop(actor);
    }

    #[test]
    fn send_too_big() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        context.start()
            .expect("start failed");

        let dst = remote_vsg_address!();
        let buffer = [0u8; crate::MAX_PACKET_SIZE + 1];
        match context.send(dst, &buffer).expect_err("send should have failed") {
            crate::error::Error::SizeTooBig => (),
            _ => assert!(false),
        }

        // Terminate gracefully
        context.send(dst, b"Foo msg")
            .expect("send failed");

        context.stop();

        drop(actor);
    }

    #[test]
    fn recv() {
        init();

        const EXPECTED_MSG: &[u8] = b"Foo msg";
        let mut buffer = [0u8; EXPECTED_MSG.len()];

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let recv_notifier = RecvNotifier::new();
        let context = super::init(valid_args!(), recv_notifier.get_callback())
            .expect("init failed");

        context.start()
            .expect("start failed");

        recv_notifier.wait(1000);

        let (src, dst, buffer) = context.recv(&mut buffer)
            .expect("recv failed");

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer, EXPECTED_MSG);

        context.stop();

        drop(actor);
    }

    fn recv_one_delay(slice_micros: u64, delay_micros: u64) {
        const EXPECTED_MSG: &[u8] = b"Foo msg";
        let mut buffer = [0u8; EXPECTED_MSG.len()];

        let actor = TestActorDesc::new("titi", |actor| send_one_delayed_msg_actor(actor, EXPECTED_MSG, slice_micros, delay_micros));

        let recv_notifier = RecvNotifier::new();
        let context = super::init(valid_args!(), recv_notifier.get_callback())
            .expect("init failed");

        context.start()
            .expect("start failed");

        recv_notifier.wait(delay_micros / 10);

        let (src, dst, buffer) = context.recv(&mut buffer)
            .expect("recv failed");
        let tv = context.gettimeofday();

        // FIXME: Use the remote address when available in the protocol
        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer, EXPECTED_MSG);
        let total_usec = tv.tv_sec as u64 * 1_000_000 + tv.tv_usec as u64;
        assert!(delay_micros <= total_usec && total_usec <= 10 * delay_micros);

        context.stop();

        drop(actor);
    }

    #[test]
    fn recv_delayed() {
        init();

        for delay_slices in 1..=100 {
            recv_one_delay(100, delay_slices * 100);
        }
    }

    #[test]
    fn recv_too_big() {
        init();

        const EXPECTED_MSG: &[u8] = b"Foo msg";
        const ORIG_BUFFER: &[u8] = b"fOO~MS";
        let mut buffer: [u8; ORIG_BUFFER.len()] = Default::default();
        buffer.copy_from_slice(ORIG_BUFFER);

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let recv_notifier = RecvNotifier::new();
        let context = super::init(valid_args!(), recv_notifier.get_callback())
            .expect("init failed");

        context.start()
            .expect("start failed");

        recv_notifier.wait(1000);

        match context.recv(&mut buffer).expect_err("recv should have failed") {
            crate::error::Error::SizeTooBig => (),
            _ => assert!(false),
        }

        assert_eq!(buffer, ORIG_BUFFER);

        context.stop();

        drop(actor);
    }

    #[test]
    fn poll() {
        init();

        const EXPECTED_MSG: &[u8] = b"Foo msg";
        let mut buffer = [0u8; EXPECTED_MSG.len()];

        let actor = TestActorDesc::new("titi", |actor| send_one_msg_actor(actor, EXPECTED_MSG));

        let context = super::init(valid_args!(), Box::new(dummy_recv_callback))
            .expect("init failed");

        context.start()
            .expect("start failed");

        while !context.poll().is_some() {
            std::thread::sleep(std::time::Duration::from_micros(1000));
        }

        let (src, dst, buffer) = context.recv(&mut buffer)
            .expect("recv failed");

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer, EXPECTED_MSG);

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

        let dst = remote_vsg_address!();
        context.send(dst, b"This is the end")
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

        let dst = remote_vsg_address!();
        context.send(dst, b"This is the end")
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

        let mut connector = context.connector.lock().unwrap();

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