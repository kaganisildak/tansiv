use buffer_pool::BufferPool;
pub(crate) use config::Config;
use connector::{Connector, ConnectorImpl, DeliverPacket, FbBuffer, MsgIn, MsgOut};
pub use error::Error;
use libc;
#[allow(unused_imports)]
use log::{debug, info, error};
use output_msg_set::{OutputMsgSet, OutputMsg};
use std::collections::LinkedList;
use std::cmp::Reverse;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;
use timer::TimerContext;
use waitfree_array_queue::WaitfreeArrayQueue;

pub const MAX_PACKET_SIZE: usize = 2048;

mod buffer_pool;
mod bytes_buffer;
mod config;
mod connector;
#[macro_use]
mod debug;
pub mod error;
mod flatbuilder_buffer;
mod output_msg_set;
mod timer;
mod vsg_address;
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
            output_msg_set::Error::NoSlotAvailable => Error::NoMemoryAvailable,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub type RecvCallback = Box<dyn Fn() -> () + Send + Sync>;
pub type DeadlineCallback = Box<dyn Fn(Duration) -> () + Send + Sync>;

// Context must be accessed concurrently from application code and the deadline handler. To
// enable this, all fields are either read-only or implement thread and signal handler-safe
// interior mutability.
pub struct Context {
    // Read-only
    address: libc::in_addr_t,
    // Read-only
    // In bits per second
    uplink_bandwidth: std::num::NonZeroUsize,
    // Read-only
    // Overhead in bytes per packet (preample, inter-frame gap...)
    uplink_overhead: usize,
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    connector: Mutex<ConnectorImpl>,
    // Concurrency: Messages are:
    // - pushed to the queue by the deadline handler,
    // - popped from the queue by application code.
    // Concurrent read-write support is provided by interior mutability.
    input_queue: WaitfreeArrayQueue<DeliverPacket>,
    // No concurrency, read-only: called only by the deadline handler
    recv_callback: RecvCallback,
    // No concurrency, read-only: called only by ::start() and the deadline handler
    deadline_callback: DeadlineCallback,
    // Concurrency:
    // - read-only by application code,
    // - read-write by the deadline handler, using interior mutability
    timer_context: TimerContext,
    // Concurrency:
    // - read/write by application code only
    last_send: Mutex<(usize, Duration, usize)>,
    // Concurrency: Buffers are:
    // - allocated and added to the set by application code,
    // - consumed and freed by the deadline handler.
    // BufferPool uses interior mutability for concurrent allocation and freeing of buffers.
    output_buffer_pool: BufferPool<FbBuffer>,
    outgoing_messages: OutputMsgSet,
    upcoming_messages: Mutex<LinkedList<OutputMsg>>,
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
    fn new(config: &Config, recv_callback: RecvCallback, deadline_callback: DeadlineCallback) -> Result<Arc<Context>> {
        let address = config.address;
        let connector = ConnectorImpl::new(config)?;
        let input_queue = WaitfreeArrayQueue::new(config.num_buffers.get());
        let timer_context = TimerContext::new(config)?;
        let output_buffer_pool = BufferPool::<FbBuffer>::new(crate::MAX_PACKET_SIZE, config.num_buffers.get());
        let outgoing_messages = OutputMsgSet::new(config.num_buffers.get());
        let upcoming_messages = LinkedList::new();

        let context = Arc::new(Context {
            address: address,
            uplink_bandwidth: config.uplink_bandwidth,
            uplink_overhead: config.uplink_overhead,
            connector: Mutex::new(connector),
            input_queue: input_queue,
            recv_callback: recv_callback,
            deadline_callback: deadline_callback,
            timer_context: timer_context,
            last_send: Mutex::new((0, Duration::ZERO, 0)),
            output_buffer_pool: output_buffer_pool,
            outgoing_messages: outgoing_messages,
            start_once: Once::new(),
            upcoming_messages: Mutex::new(upcoming_messages),
        });
        timer::register(&context)?;

        Ok(context)
    }

    pub fn start(&self) -> Result<chrono::Duration> {
        let mut res = Err(Error::AlreadyStarted);

        self.start_once.call_once(|| res = (|| {
            let mut connector = self.connector.lock().unwrap();
            let msg = connector.recv()?;
            deadline_handler_debug!("Context::start() received msg = {:?}", msg);
            // The deadline handler can fire and try to lock connector at any time once self.0.start()
            // is called so we must unlock connector before.
            drop(connector);
            match msg {
                // Writing Ok(...?) helps the compiler to know how to convert std::io::Error to Error
                MsgIn::GoToDeadline(deadline) => {
                    (self.deadline_callback)(deadline);
                    Ok(self.timer_context.start(deadline)?)
                },
                _ => Err(Error::ProtocolViolation),
            }
        })());

        if res.is_err() {
            error!("Context::start() failed: {:?}", res);
        }
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
        deadline_handler_debug!("Context::at_deadline() current_deadline = {:?}", current_deadline);
        for mut send_packet_builder in messages {
            let mut send_time = send_packet_builder.send_time();
            send_time = self.timer_context.convert_timestamp(send_time);

            // It is possible that messages are timestamped after a deadline with KVM.
            // It can only happen when the delay of the network card emulation
            // exceeds a deadline.
            // Indeed as the vCPU thread is the one performing the timestamp, it is not
            // possible to have a situation where a deadline is handled at the same
            // time as the timestamp is taken (in which case the solution would be
            // more complex).
            if let Some(send_time_overrun) = self.timer_context.check_deadline_overrun(send_time, &self.upcoming_messages) {
                let mut upcoming_messages = self.upcoming_messages.lock().unwrap();
                send_packet_builder.set_send_time(send_time_overrun);
                upcoming_messages.push_back(send_packet_builder);
                drop(upcoming_messages);
                continue;
            }

            // FIXME(msimonin): the trait `InnerBufferDisplay` is not implemented for `flatbuilder_buffer::FbBuilder<'static, connector::InFbInitializer>`
            deadline_handler_debug!("Context::at_deadline() message to send (send_time = {:?}, src = {}, dst = {})",
                send_time,
                vsg_address::to_ipv4addr(send_packet_builder.src()),
                vsg_address::to_ipv4addr(send_packet_builder.dst()));

            let send_time = if send_time < previous_deadline {
                // This message was time-stamped before the previous deadline but inserted after.
                // Fix the timestamp to stay between the deadlines.
                deadline_handler_debug!("Context::at_deadline() fixing send_time to {:?}", previous_deadline);
                previous_deadline
            } else {
                if send_time > current_deadline {
                    // The kernel was too slow to fire the timer...
                    error!("send_time = {:?} is beyond current_deadline = {:?}! Aborting", send_time, current_deadline);
                    return AfterDeadline::EndSimulation;
                }
                send_time
            };
            // so, the payload is a Buffer<FbBuffer> partially built with the actual payload inside
            // we finish the construction here and send it over the wire
            if let Err(_e) = connector.send(MsgOut::SendPacket(send_packet_builder.finish(send_time))) {
                error!("send(SendPacket) failed: {}", _e);
                return AfterDeadline::EndSimulation;
            }
        }

        // Now, check if there are any messages that were timestamped after a
        // deadline that are ready to be sent.
        let mut upcoming_messages = self.upcoming_messages.lock().unwrap();
        while !upcoming_messages.is_empty() && upcoming_messages.front().unwrap().send_time() <= current_deadline {
            let message = upcoming_messages.pop_front().unwrap();
            let send_time = message.send_time();
            deadline_handler_debug!("Context::at_deadline() message to send (send_time = {:?}, src = {}, dst = {})",
                send_time,
                vsg_address::to_ipv4addr(message.src()),
                vsg_address::to_ipv4addr(message.dst()));

            if let Err(_e) = connector.send(MsgOut::SendPacket(message.finish(send_time))) {
                error!("send(SendPacket) failed: {}", _e);
                return AfterDeadline::EndSimulation;
            }
        }
        drop(upcoming_messages);

        // Second, notify that we reached the deadline
        deadline_handler_debug!("Context::at_deadline() sending AtDeadline");
        if let Err(_e) = connector.send(MsgOut::AtDeadline) {
            error!("send(AtDeadline) failed: {}", _e);
            return AfterDeadline::EndSimulation;
        }

        // Third, receive messages from others, followed by next deadline
        let input_queue = &self.input_queue;
        let may_notify = input_queue.is_empty();
        deadline_handler_debug!("Context::at_deadline() may_notify = {}", may_notify);

        let after_deadline = loop {
            let msg = connector.recv();
            match msg {
                Ok(msg) => if let Some(after_deadline) = self.handle_actor_msg(msg) {
                    break after_deadline;
                },
                Err(_e) => {
                    error!("recv failed: {}", _e);
                    break AfterDeadline::EndSimulation;
                }
            }
        };

        if may_notify && !input_queue.is_empty() {
            deadline_handler_debug!("Context::at_deadline() calling recv_callback()");
            (self.recv_callback)();
        }

        if let AfterDeadline::NextDeadline(deadline) = after_deadline {
            (self.deadline_callback)(deadline);
        }

        deadline_handler_debug!("Context::at_deadline() after_deadline = {:?}", after_deadline);
        after_deadline
    }

    fn handle_actor_msg(&self, msg: MsgIn) -> Option<AfterDeadline> {
        deadline_handler_debug!("Context::handle_actor_msg() received msg = {}", msg);
        match msg {
            MsgIn::DeliverPacket(d) => {
                let src = d.src();
                let size = d.payload().len();
                if self.input_queue.push(d).is_err() {
                    info!("Dropping input packet from {} of {} bytes", src, size);
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
        let mut send_time = self.timer_context.simulation_now();

        let mut last_send = self.last_send.lock().unwrap();
        let (last_send_size, last_send_time, mut delayed_count) = *last_send;
        let next_send_floor = last_send_time + Duration::from_nanos(((last_send_size + self.uplink_overhead) * 8 * 1_000_000_000 / usize::from(self.uplink_bandwidth)) as u64);
        let delay = next_send_floor.saturating_sub(send_time);
        if !delay.is_zero() {
            self.timer_context.delay(delay);

            send_time = next_send_floor;
            delayed_count += 1;
        }

        *last_send = (msg.len(), send_time, delayed_count);
        drop(last_send);

        // It is possible that the deadline is reached just after recording the send time and
        // before inserting the message, which leads to sending the message at the next deadline.
        // This would violate the property that send times must be after the previous deadline
        // (included) and (strictly) before the current deadline. To solve this, ::at_deadline()
        // takes the latest time between the recorded time and the previous deadline.

        // There's an hidden check behind this allocation that the msg len isn't too big
        let buffer = match self.output_buffer_pool.allocate_buffer(msg.len()) {
            Ok(b) => b,
            Err(e) => {
                error!("send error at send_time {:?}: {:?}", send_time, e);
                return Err(e.into());
            }
        };

        self.outgoing_messages.insert(OutputMsg::new(self.address, dst, send_time, msg, buffer)?)?;

        if !delay.is_zero() {
            // info!("send_time {} changed: next_deadline {}, delayed_count {}, capped_count {}", send_time, next_deadline, delayed_count, capped_count);
            info!("send_time {:?} changed: +{:?}, delayed_count {}", send_time, delay, delayed_count);
        }

        debug!("new packet: send_time = {:?}, src = {}, dst = {}, size = {}", send_time, vsg_address::to_ipv4addr(self.address), vsg_address::to_ipv4addr(dst), msg.len());

        Ok(())
    }

    pub fn recv<'a, 'b>(&'a self, msg: &'b mut [u8]) -> Result<(libc::in_addr_t, libc::in_addr_t, &'b mut [u8])> {
        match self.input_queue.pop() {
            Some(msg_in) => {
                if msg.len() >= msg_in.payload().len() {
                    let msg = &mut msg[..msg_in.payload().len()];
                    msg.copy_from_slice(&msg_in.payload());
                    Ok((msg_in.src(), msg_in.dst(), msg))
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

pub fn init<I>(args: I, recv_callback: RecvCallback, deadline_callback: DeadlineCallback) -> Result<Arc<Context>>
    where I: IntoIterator,
          I::Item: Into<std::ffi::OsString> + Clone {
    use structopt::StructOpt;

    #[cfg(all(feature = "use-own-logger", not(any(test, feature = "test-helpers"))))] {
        // Allow multiple contexts to be created in a same process
        static INIT: std::sync::Once = std::sync::Once::new();
        let mut ret = Ok(());

        INIT.call_once(|| ret = simple_logger::SimpleLogger::from_env().init().or_else(|e| Err(std::io::Error::new(std::io::ErrorKind::Other, e))));
        ret
    }?;

    let config = Config::from_iter_safe(args).or_else(|e| Err(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    debug!("{:?}", config);

    Context::new(&config, recv_callback, deadline_callback)
}

#[cfg(any(test, feature = "test-helpers"))]
#[macro_use]
pub mod test_helpers {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;
    use super::connector::{MsgIn, MsgOut, create_deliver_packet_unprefixed};
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
            &["-atiti", "-n", local_vsg_address_str!(), "-w100000000", "-x24", "-t1970-01-01T00:00:00"]
        }
    }

    #[macro_export]
    macro_rules! valid_args_h1 {
        () => {
            &["-atiti", "-n", local_vsg_address_str!(), "-w100000000", "-x24", "-t1970-01-01T01:00:00"]
        }
    }

    #[macro_export]
    macro_rules! invalid_args {
        () => {
            &["-btiti", "-n", local_vsg_address_str!(), "-w100000000", "-x24", "-t1970-01-01T00:00:00"]
        }
    }

    pub fn dummy_recv_callback() -> () {
    }

    pub fn dummy_deadline_callback(_deadline: Duration) -> () {
    }

    pub const START_ACTOR_DEADLINE: Duration = Duration::from_nanos(100000);

    pub fn start_actor(actor: &mut TestActor) -> TestResult<()> {
        actor.send(MsgIn::GoToDeadline(START_ACTOR_DEADLINE))?;
        actor.send(MsgIn::EndSimulation)
    }

    pub const RECV_ONE_MSG_ACTOR_SLICE: Duration = Duration::from_micros(100);

    // Actor that will let the VM run until the VM explicitly stops, by either sending a packet
    // (clean stop) or just closing the connection (reported as an error without making the test
    // fail)
    pub fn recv_one_msg_actor(actor: &mut TestActor) -> TestResult<()> {
        let mut deadline = Duration::from_micros(0);
        loop {
            deadline += RECV_ONE_MSG_ACTOR_SLICE;
            actor.send(MsgIn::GoToDeadline(deadline))?;
            let msg = actor.recv()?;
            match msg {
                MsgOut::AtDeadline => (),
                MsgOut::SendPacket(_) => break,
            }
        }
        actor.send(MsgIn::EndSimulation)
    }

    const SEND_ONE_MSG_ACTOR_DELAY_MICROS: u64 = 100;
    pub const SEND_ONE_MSG_ACTOR_DELAY: Duration = Duration::from_micros(SEND_ONE_MSG_ACTOR_DELAY_MICROS);

    pub fn send_one_msg_actor(actor: &mut TestActor, msg: &[u8]) -> TestResult<()> {
        send_one_delayed_msg_actor(actor, msg, SEND_ONE_MSG_ACTOR_DELAY_MICROS, SEND_ONE_MSG_ACTOR_DELAY_MICROS)
    }

    pub fn send_one_delayed_msg_actor(actor: &mut TestActor, msg: &[u8], slice_micros: u64, delay_micros: u64) -> TestResult<()> {
       //(&mut buffer).copy_from_slice(msg);

        let mut next_deadline_micros = slice_micros;
        while next_deadline_micros < delay_micros {
            actor.send(MsgIn::GoToDeadline(Duration::from_micros(next_deadline_micros)))?;
            loop {
                match actor.recv()? {
                    MsgOut::AtDeadline => break,
                    _ => (),
                }
            }

            next_deadline_micros += slice_micros;
        }

        actor.send(MsgIn::GoToDeadline(Duration::from_micros(delay_micros)))?;
        let src = local_vsg_address!();
        let dst = remote_vsg_address!();

        let mut builder = flatbuffers::FlatBufferBuilder::new();
        create_deliver_packet_unprefixed(&mut builder, src, dst, msg);
        let fb = builder.finished_data();
        let size = fb.len();
        let buffer_pool = crate::BufferPool::<crate::bytes_buffer::BytesBuffer>::new(size, 1);
        let mut buffer = TestActor::check(buffer_pool.allocate_buffer(size), "Buffer allocation failed")?;

        let _ = &mut buffer.copy_from_slice(fb);


        actor.send(MsgIn::new_deliver_packet(buffer).unwrap())?;
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

    struct DeadlineNotifierData {
        deadline: seq_lock::SeqLock<Duration>,
        num_called: AtomicUsize,
    }

    #[derive(Clone)]
    pub struct DeadlineNotifier(Arc<DeadlineNotifierData>);

    impl DeadlineNotifier {
        pub const INITIAL_DEADLINE: Duration = Duration::from_secs(0);

        pub fn new() -> DeadlineNotifier {
            DeadlineNotifier(Arc::new(DeadlineNotifierData {
                deadline: seq_lock::SeqLock::new(Self::INITIAL_DEADLINE),
                num_called: AtomicUsize::new(0),
            }))
        }

        pub fn notify(&self, deadline: Duration) -> () {
            self.0.deadline.write(|_| deadline);
            self.0.num_called.fetch_add(1, Ordering::SeqCst);
        }

        pub fn get_callback(&self) -> crate::DeadlineCallback {
            let cb_notifier = self.clone();
            Box::new(move |deadline| cb_notifier.notify(deadline))
        }

        pub fn deadline(&self) -> Duration {
            self.0.deadline.read(|d| d)
        }

        pub fn num_called(&self) -> usize {
            self.0.num_called.load(Ordering::SeqCst)
        }
    }

    static INIT: std::sync::Once = std::sync::Once::new();

    pub fn init() {
        // Cargo test runs all tests in a same process so don't confuse log by setting a logger
        // several times.
        INIT.call_once(|| simple_logger::SimpleLogger::from_env().init().unwrap());
    }
}

#[cfg(test)]
mod test {
    #[allow(unused_imports)]
    use log::{error, info};
    use std::time::Duration;
    use super::connector::Connector;
    use super::{connector::test_helpers::*, test_helpers::*};

    #[test]
    fn init_valid() {
        init();

        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        super::init(valid_args!(), Box::new(dummy_recv_callback), Box::new(dummy_deadline_callback))
            .expect("init failed");

        // assert_eq!(chrono::NaiveDateTime::from_timestamp(0, 0), context.0.simulation_offset);

        drop(actor);
    }

    #[test]
    fn init_invalid() {
        init();

        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        super::init(invalid_args!(), Box::new(dummy_recv_callback), Box::new(dummy_deadline_callback))
            .expect_err("init returned a context");

        drop(actor);
    }

    #[test]
    fn start_stop() {
        init();

        let actor = TestActorDesc::new("titi", start_actor);
        let deadline_notifier = DeadlineNotifier::new();
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback), deadline_notifier.get_callback())
            .expect("init failed");
        assert_eq!(DeadlineNotifier::INITIAL_DEADLINE, deadline_notifier.deadline());
        assert_eq!(0, deadline_notifier.num_called());

        let offset = context.start()
            .expect("start failed");
        assert_eq!(chrono::Duration::zero(), offset);

        context.stop();

        drop(actor);
    }

    #[test]
    fn start_already() {
        init();

        let actor = TestActorDesc::new("titi", start_actor);
        let deadline_notifier = DeadlineNotifier::new();
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback), deadline_notifier.get_callback())
            .expect("init failed");

        context.start()
            .expect("start failed");

        match context.start().expect_err("start should have failed") {
            super::error::Error::AlreadyStarted => (),
            _ => assert!(false),
        }
        assert_eq!(1, deadline_notifier.num_called());

        context.stop();

        drop(actor);
    }

    #[test]
    fn send() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let deadline_notifier = DeadlineNotifier::new();
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback), deadline_notifier.get_callback())
            .expect("init failed");

        context.start()
            .expect("start failed");

        let dst = remote_vsg_address!();
        context.send(dst, b"Foo msg")
            .expect("send failed");

        context.stop();
        let num_called = deadline_notifier.num_called();
        assert!(num_called > 0);
        assert_eq!((num_called as u32) * RECV_ONE_MSG_ACTOR_SLICE, deadline_notifier.deadline());

        drop(actor);
    }

    #[test]
    fn send_too_big() {
        init();

        let actor = TestActorDesc::new("titi", recv_one_msg_actor);
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback), Box::new(dummy_deadline_callback))
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
        let deadline_notifier = DeadlineNotifier::new();
        let context = super::init(valid_args!(), recv_notifier.get_callback(), deadline_notifier.get_callback())
            .expect("init failed");

        context.start()
            .expect("start failed");

        recv_notifier.wait(1000);

        let (src, dst, buffer) = context.recv(&mut buffer)
            .expect("recv failed");

        assert_eq!(src, local_vsg_address!());
        assert_eq!(dst, remote_vsg_address!());
        assert_eq!(buffer, EXPECTED_MSG);

        assert_eq!(1, deadline_notifier.num_called());
        assert_eq!(SEND_ONE_MSG_ACTOR_DELAY, deadline_notifier.deadline());

        context.stop();

        drop(actor);
    }

    fn recv_one_delay(slice_micros: u64, delay_micros: u64) {
        const EXPECTED_MSG: &[u8] = b"Foo msg";
        let mut buffer = [0u8; EXPECTED_MSG.len()];

        let actor = TestActorDesc::new("titi", |actor| send_one_delayed_msg_actor(actor, EXPECTED_MSG, slice_micros, delay_micros));

        let recv_notifier = RecvNotifier::new();
        let deadline_notifier = DeadlineNotifier::new();
        let context = super::init(valid_args!(), recv_notifier.get_callback(), deadline_notifier.get_callback())
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
        assert!(delay_micros <= total_usec, "Message received too early: before {}us instead of after {}us", total_usec, delay_micros);

        if total_usec <= 10 * delay_micros {
            error!("Message received really too late: short before {}us instead of short after {}us", total_usec, delay_micros);
        }

        assert_eq!((delay_micros / slice_micros) as usize, deadline_notifier.num_called());
        assert_eq!(Duration::from_micros(delay_micros), deadline_notifier.deadline());

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
        let context = super::init(valid_args!(), recv_notifier.get_callback(), Box::new(dummy_deadline_callback))
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

        let context = super::init(valid_args!(), Box::new(dummy_recv_callback), Box::new(dummy_deadline_callback))
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
        let context = super::init(valid_args!(), Box::new(dummy_recv_callback), Box::new(dummy_deadline_callback))
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
        let context = super::init(valid_args_h1!(), Box::new(dummy_recv_callback), Box::new(dummy_deadline_callback))
            .expect("init failed");

        let offset = context.start()
            .expect("start failed");
        assert_eq!(chrono::Duration::hours(1), offset);

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
        let context = super::init(valid_args_h1!(), Box::new(dummy_recv_callback), Box::new(dummy_deadline_callback))
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
