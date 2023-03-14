// Use chrono::Duration (re-exported from time::Duration) to allow negative values, which are not
// allowed in std::time::Duration
use chrono::{Duration, NaiveDateTime};
use lazy_static::lazy_static;
use libc_timer::{clock, timer, ClockId};
use seq_lock::SeqLock;
use std::io::Result;
use std::sync::{Arc, Mutex, RwLock, Weak};
#[cfg(not(any(test, feature = "test-helpers")))]
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration as StdDuration;
use crate::Context;

#[derive(Debug)]
struct AdjustedTime(SeqLock<Duration>);

impl AdjustedTime {
    fn new(offset: Duration) -> AdjustedTime {
        AdjustedTime(SeqLock::new(offset))
    }

    fn get<F, T>(&self, f: F) -> T
        where F: Fn(Duration) -> T {
        self.0.read(f)
    }

    fn adjust<F>(&self, f: F) -> ()
        where F: Fn(Duration) -> Duration {
        self.0.write(f)
    }
}

#[derive(Debug)]
pub struct TimerContext {
    // Offset from simulation time to application time
    // Concurrency: RO
    time_offset: Duration,
    // Object to get and adjust the local application time
    // Concurrency:
    // - read by application code
    // - written by the deadline handler
    application_time: AdjustedTime,
    // Object to get and adjust the global simulation time
    // Concurrency:
    // - read by application code
    // - written by the deadline handler
    simulation_time: AdjustedTime,
    // Handle to the system timer used to schedule deadlines
    // Concurrency: RO
    timer_id: timer::TimerId,
    // True when the deadline handler is running, indicating that local simulation time should not progress
    // Concurrency:
    // - read by application code
    // - written by the deadline handler
    at_deadline: AtomicBool,
    // Constant time during the deadline handling in application time
    // No concurrency:
    // - read by application code but only when called by the deadline handler
    // - written by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    current_deadline: Mutex<NaiveDateTime>,
    // Previous deadline in global simulation time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    prev_deadline: Mutex<StdDuration>,
    // Next deadline in global simulation time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    next_deadline: Mutex<StdDuration>,
    ///////////////////// Next fields for DEBUG only
    // Previous deadline in raw monotonic time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    prev_deadline_raw: Mutex<StdDuration>,
    // Next deadline in raw monotonic time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    next_deadline_raw: Mutex<StdDuration>,
    // Stop flag to synchronize on last timer expiration
    // Concurrency:
    // - read by ::stop() in application context
    // - written by the deadline handler
    stopped: AtomicBool,
}

impl TimerContext {
    const CLOCK: ClockId = ClockId::Monotonic;
    // If different from SIGALRM, be sure to give an appropriate SigevNotify to libc_timer::timer::create()
    const DEADLINE_SIG: nix::sys::signal::Signal = nix::sys::signal::Signal::SIGALRM;

    pub(crate) fn new(config: &crate::Config) -> Result<TimerContext> {
        use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet};

        let time_offset = config.time_offset.signed_duration_since(NaiveDateTime::from_timestamp(0, 0));

        let application_time = AdjustedTime::new(Duration::zero());
        let simulation_time = AdjustedTime::new(Duration::zero());

        // This sighandler init is idempotent if successful.
        let action = SigAction::new(SigHandler::Handler(deadline_handler), SaFlags::SA_RESTART, SigSet::all());
        unsafe {
            sigaction(Self::DEADLINE_SIG, &action).or_else(|e| Err(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        }

        let timer_id = timer::create(Self::CLOCK, None)?;

        // Time starts at 0 in global simulation time.
        let prev_deadline = Mutex::new(StdDuration::new(0, 0));
        let next_deadline = Mutex::new(StdDuration::new(0, 0));
        let prev_deadline_raw = Mutex::new(StdDuration::new(0, 0));
        let next_deadline_raw = Mutex::new(StdDuration::new(0, 0));

        Ok(TimerContext {
            time_offset: time_offset,
            application_time: application_time,
            simulation_time: simulation_time,
            timer_id: timer_id,
            at_deadline: AtomicBool::new(true),
            current_deadline: Mutex::new(config.time_offset),
            prev_deadline: prev_deadline,
            next_deadline: next_deadline,
            prev_deadline_raw: prev_deadline_raw,
            next_deadline_raw: next_deadline_raw,
            stopped: AtomicBool::new(true),
        })
    }

    fn freeze_time(&self) -> StdDuration {
        let now = clock::gettime(Self::CLOCK).unwrap();
        deadline_handler_debug!("TimerContext::freeze_time() system time = {:?}", now);
        *self.current_deadline.lock().unwrap() = self.application_now();
        self.at_deadline.store(true, Ordering::Release);
        deadline_handler_debug!("TimerContext::freeze_time() jitter (now - next_deadline_raw) = {}", Duration::from_std(now).unwrap() - Duration::from_std(*self.next_deadline_raw.lock().unwrap()).unwrap());
        now
    }

    fn thaw_time_to_deadline(&self, freeze_time: Option<StdDuration>, deadline: StdDuration) -> Result<()> {
        let mut next_deadline = self.next_deadline.lock().unwrap();
        let next_deadline_val = *next_deadline;
        *self.prev_deadline.lock().unwrap() = next_deadline_val;
        *next_deadline = deadline;
        deadline_handler_debug!("TimerContext::thaw_time_to_deadline() set next_deadline = {:?}", next_deadline);
        // First call can be interrupted by the signal handler and deadlock
        drop(next_deadline);

        let now = clock::gettime(Self::CLOCK).unwrap();
        deadline_handler_debug!("TimerContext::thaw_time_to_deadline() system time = {:?}", now);
        let new_next_deadline_raw = now + (deadline - next_deadline_val);

        // DEBUG only
        let mut next_deadline_raw = self.next_deadline_raw.lock().unwrap();
        *self.prev_deadline_raw.lock().unwrap() = *next_deadline_raw;
        *next_deadline_raw = new_next_deadline_raw;
        // First call can be interrupted by the signal handler and deadlock
        drop(next_deadline_raw);
        // **********

        if let Some(freeze_time) = freeze_time {
            let elapsed_time = Duration::from_std(now - freeze_time).unwrap();
            deadline_handler_debug!("TimerContext::thaw_time_to_deadline() elapsed_time = {}", elapsed_time);

            self.application_time.adjust(|offset| offset - elapsed_time);
            self.simulation_time.adjust(|offset| offset - elapsed_time);
        } else {
            // Here current_deadline == config.time_offset
            let current_deadline = *self.current_deadline.lock().unwrap();
            // Here is where the application time reference is recorded
            let local_now = chrono::offset::Local::now().naive_local();
            deadline_handler_debug!("TimerContext::thaw_time_to_deadline() application time offset = {}", current_deadline - local_now);
            self.application_time.adjust(|_| current_deadline - local_now);

            // Time starts at 0 in global simulation time.
            deadline_handler_debug!("TimerContext::thaw_time_to_deadline() simulation time offset = -{:?}", now);
            self.simulation_time.adjust(|_| -Duration::from_std(now).unwrap());
        }

        self.at_deadline.store(false, Ordering::Release);

        // The first call of ::thaw_time_to_deadline() is not in signal handler context and can be
        // interrupted by the signal handler, so make sure that everything is already up to date
        deadline_handler_debug!("TimerContext::thaw_time_to_deadline() setting timer to fire at {:?}", new_next_deadline_raw);
        timer::settime(self.timer_id, timer::SettimeFlags::AbsoluteTime, None, new_next_deadline_raw)?;
        Ok(())
    }

    pub fn start(&self, deadline: StdDuration) -> Result<Duration> {
        self.stopped.store(false, Ordering::Release);
        match self.thaw_time_to_deadline(None, deadline) {
            Ok(_) => Ok(self.time_offset),
            Err(e) => {
                self.stopped.store(true, Ordering::Release);
                Err(e)
            },
        }
    }

    pub fn stop(&self) {
        // Hope it will not last too long
        // Otherwise we should loop on (sigprocmask(SIGALRM), check, unmask)
        while !self.stopped.load(Ordering::Acquire) {}
    }

    /// Returns the application local time adjusted to compensate simulation delays
    pub fn application_now(&self) -> NaiveDateTime {
        if !self.at_deadline.load(Ordering::Acquire) {
            self.application_time.get(|offset| chrono::offset::Local::now().naive_local() + offset)
        } else {
            *self.current_deadline.lock().unwrap()
        }
    }

    /// Returns the global simulation time
    pub fn simulation_now(&self) -> StdDuration {
        if !self.at_deadline.load(Ordering::Acquire) {
            self.simulation_time.get(|offset| (Duration::from_std(clock::gettime(Self::CLOCK).unwrap()).unwrap() + offset).to_std().unwrap())
        } else {
            panic!("simulation_now() called while handling deadline")
        }
    }

    pub fn simulation_previous_deadline(&self) -> StdDuration {
        assert!(self.at_deadline.load(Ordering::Relaxed));
        *self.prev_deadline.lock().unwrap()
    }

    pub fn simulation_next_deadline(&self) -> StdDuration {
        // We have to access the next deadline even when we are handling
        // a deadline for the mechanism that fixes messages timestamped late.
        // Should not break anything as next_deadline is only modified while
        // handling a deadline.
        // assert!(self.at_deadline.load(Ordering::Relaxed));
        *self.next_deadline.lock().unwrap()
    }
}

impl Drop for TimerContext {
    fn drop(&mut self) {
        timer::delete(self.timer_id);
        // Do not reset the sighandler for DEADLINE_SIG, other TimerContext might still need it
        // The sighandler will do nothing if no TimerContext is alive.
        // FIXME: Count the number of use of the sighandler to properly reset it
    }
}

lazy_static! {
    static ref CONTEXT: RwLock<Weak<Context>> = RwLock::new(Weak::new());
}
#[cfg(not(any(test, feature = "test-helpers")))]
static INIT: Once = Once::new();

#[cfg(not(any(test, feature = "test-helpers")))]
pub fn register(context: &Arc<Context>) -> Result<()> {
    let mut success = false;

    INIT.call_once(|| {
        // Signal handler safety: This is the only place where CONTEXT is write-locked.
        let mut uniq_context = CONTEXT.write().unwrap();
        *uniq_context = Arc::downgrade(&context);
        success = true;
    });

    if success {
        Ok(())
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "Implementation only supports a single context"))
    }
}

// Let individual tests overwrite each other's context in turn.
// Assumes that tests are run in a single thread
#[cfg(any(test, feature = "test-helpers"))]
pub fn register(context: &Arc<Context>) -> Result<()> {
    let mut uniq_context = CONTEXT.write().unwrap();
    *uniq_context = Arc::downgrade(&context);
    Ok(())
}

extern "C" fn deadline_handler(_: libc::c_int) {
    use crate::AfterDeadline;

    deadline_handler_debug!("deadline_handler() called");
    if let Some(context) = CONTEXT.read().unwrap().upgrade() {
        let freeze_time = context.timer_context.freeze_time();
        match context.at_deadline() {
            AfterDeadline::NextDeadline(deadline) => {
                context.timer_context.thaw_time_to_deadline(Some(freeze_time), deadline).expect("thaw_time_to_deadline failed")
            },
            AfterDeadline::EndSimulation => context.timer_context.stopped.store(true, Ordering::Release),
        }
    }
}
