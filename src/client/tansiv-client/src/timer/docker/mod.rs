use chrono::Duration;
use std::io::Result;
use std::io::Read;
use std::collections::LinkedList;
use std::ops::Deref;
use std::pin::Pin;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration as StdDuration;
use std::os::unix::net::UnixStream;
use std::os::fd::AsRawFd;
use std::os::fd::RawFd;

use crate::output_msg_set::{OutputMsg};

use core::arch::x86_64::{_rdtsc};

use log::debug;

#[derive(Debug)]
struct StopperContext {
    // Socket used for communication with container stopper process.
    stopper_stream: UnixStream,
    // Child structure of spawned stopper process
    stopper_process: std::process::Child,
    // Time offset (shared memory)
    offset: crate::docker::SharedTimespec,
}

#[derive(Debug)]
struct DockerConfigElements {
    sequence_number: u32,
    container_id: String,
}
impl From<&crate::Config> for DockerConfigElements {
    fn from(item: &crate::Config) -> Self {
        Self {
            sequence_number: item.docker_sequence_number,
            container_id: item.docker_container_id.clone(),
        }
    }
}

#[derive(Debug)]
pub struct TimerContextInner {
    context: Mutex<Weak<crate::Context>>,
    // Previous deadline in global simulation time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    prev_deadline: Mutex<StdDuration>,
    // Next deadline in global simulation time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    next_deadline: Mutex<StdDuration>,

    config: DockerConfigElements,
    stopper: Mutex<Option<StopperContext>>,
    // std::time::Instant used to get current simulation time (this + prev_deadline)
    last_thawing_instant: Mutex<std::time::Instant>,
}

// Wrapper struct to avoid conflicts between Pin::new() and TimerContextInner::new()
#[derive(Debug)]
pub struct TimerContext(Pin<Arc<TimerContextInner>>);

impl TimerContext {
    pub(crate) fn new(config: &crate::Config) -> Result<TimerContext> {
        Ok(TimerContext(Arc::pin(TimerContextInner::new(config))))
    }
}

impl Deref for TimerContext {
    type Target = Pin<Arc<TimerContextInner>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TimerContextInner {
    fn new(config : &crate::Config) -> TimerContextInner {
        let context = Mutex::new(Weak::new());
        let prev_deadline = Mutex::new(Default::default());
        let next_deadline = Mutex::new(StdDuration::new(0, 0));
        let stopper = Mutex::new(None);
        let last_thawing_instant = Mutex::new(std::time::Instant::now());

        TimerContextInner {
            context,
            prev_deadline,
            next_deadline,
            config: config.into(),
            stopper,
            last_thawing_instant,
        }
    }

    pub fn set_next_deadline(self: &Pin<Arc<Self>>, deadline: StdDuration) {
        let next_deadline_val = *self.next_deadline.lock().unwrap();

        let timer_deadline = deadline - next_deadline_val;
        *self.prev_deadline.lock().unwrap() = next_deadline_val;
        *self.next_deadline.lock().unwrap() = deadline;

        let mut stopper = self.stopper.lock().unwrap();
        let mut some_stopper = stopper.as_mut().expect("set_next_deadline called before start");
        let mut last_thawing_instant = self.last_thawing_instant.lock().unwrap();

        crate::docker::write_timespec(&timer_deadline, &mut some_stopper.stopper_stream).expect("communication with stopper failed");
        *last_thawing_instant = std::time::Instant::now();
    }

    pub fn start(self: &Pin<Arc<Self>>, deadline: StdDuration) -> Result<Duration> {
        let mut stopper = self.stopper.lock().unwrap();
        // Make sure ::start() is not called again before ::stop()
        if !stopper.is_none() {
            panic!("TimerContextInner::start called twice");
        }

        // TODO: should probably try to avoid unwrap here
        let (stopper_process, stopper_stream) = crate::docker::start_stopper(self.config.sequence_number, &self.config.container_id).unwrap();
        //let offset = crate::docker::wait_and_mmap_offset(self.config.sequence_number).unwrap();
        let offset = crate::docker::mmap_offset(self.config.sequence_number).unwrap();
        *stopper = Some(StopperContext{
            stopper_stream,
            stopper_process,
            offset,
        });

        std::mem::drop(stopper);
        self.set_next_deadline(deadline);

        Ok(Duration::zero())
    }

    // TODO: Currently unsafe! Assumes that start() has been called before and that stop() is never
    // called twice. Otherwise calling stop() prematurately drops self!
    pub fn stop(self: &Pin<Arc<Self>>) {
        let mut stopper = self.stopper.lock().unwrap();
        if stopper.is_none() {
            panic!("TimerContextInner::start called before start");
        }
        let mut some_stopper = stopper.as_mut().unwrap();
        some_stopper.stopper_stream.shutdown(std::net::Shutdown::Both);
        some_stopper.stopper_process.wait().unwrap();

        // Safety: TODO
        // Drop the reference given to qemu_timer
        // It is easier to use the opaque pointer from self than using the one from qemu_timer
        let ptr = self.deref() as *const TimerContextInner;
        // Safety: Arc::from_raw() gets back the ManuallyDrop'ed reference given by Arc::clone() in
        // ::start()
        unsafe {
            drop(Arc::from_raw(ptr));
        }
    }

    /// Returns the application local time adjusted to compensate simulation delays
    pub fn application_now(&self) -> chrono::NaiveDateTime {
        // TODO: Better feature-gate gettimeofday() and friends
        // Useless with Docker
        unimplemented!()
    }

    /// Returns the global simulation time
    pub fn simulation_now(&self) -> StdDuration {
        // TODO: assumes simulation is RUNNING when this is called, check if true
        let prev_deadline = self.prev_deadline.lock().unwrap();
        let last_thawing_instant = self.last_thawing_instant.lock().unwrap();
        return last_thawing_instant.elapsed()+*prev_deadline;
    }

    pub fn simulation_previous_deadline(&self) -> StdDuration {
        *self.prev_deadline.lock().unwrap()
    }

    pub fn simulation_next_deadline(&self) -> StdDuration {
        *self.next_deadline.lock().unwrap()
    }

    pub fn check_deadline_overrun(&self, send_time: StdDuration, list: &Mutex<LinkedList<OutputMsg>>) -> Option<StdDuration> {
        if send_time > self.simulation_next_deadline() {
            let upcoming_messages = list.lock().unwrap();
            // It is possible that this message is timestamped before messages
            // that are already in the Min-Heap.
            // It is possible because the delay of the network card emulation is
            // variable, and of the time adjustments to the VM clock after a
            // deadline.
            // If this happens, change the timestamp of the message to be the
            // same as the last one in the list
            if let Some(last_msg) = upcoming_messages.back() {
                if last_msg.send_time() > send_time {
                    deadline_handler_debug!("Message timestamped {:?} before another message!\n", last_msg.send_time() - send_time);
                    return Some(last_msg.send_time());
                }   
            }
            return Some(send_time);
        }
        return None;
    }

    pub fn get_stopper_fd(&self) -> RawFd {
        self.stopper.lock().unwrap().as_ref().unwrap().stopper_stream.as_raw_fd()
    }

    pub fn flush_one_stopper_byte(&self) -> bool {
        let binding = self.stopper.lock().unwrap();
        let mut stopper_stream = &binding.as_ref().unwrap().stopper_stream;
        //TODO: copy-pasted from docker/mod.rs, make a function
        loop {
            let mut buf : [u8; 1] = Default::default();
            match stopper_stream.read(&mut buf) {
                Ok(1) => break,
                Ok(0) => return false,
                Err(e) => match e.kind() {
                    std::io::ErrorKind::Interrupted => (),
                    _ => return false,
                },
                _ => return false,
            }
        }

        return true;
    }
}

pub fn register(context: &Arc<crate::Context>) -> Result<()> {
    let timer_context = &context.timer_context;
    *timer_context.context.lock().unwrap() = Arc::downgrade(context);
    Ok(())
}
