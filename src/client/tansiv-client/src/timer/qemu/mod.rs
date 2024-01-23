use chrono::Duration;
use qemu_timer_sys::{QEMUClockType, qemu_clock_get_ns};
use std::collections::VecDeque;
use std::mem::MaybeUninit;
use std::io::Result;
use std::marker::PhantomPinned;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration as StdDuration;

use crate::output_msg_set::{OutputMsg};

#[repr(transparent)]
struct PollSendCallback(Arc<crate::PollSendCallback>);

impl std::fmt::Debug for PollSendCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("PollSendCallback: {:?}", Arc::as_ptr(&self.0)))
    }
}

#[derive(Debug)]
pub struct TimerContextInner {
    qemu_timer: Mutex<MaybeUninit<qemu_timer_sys::QEMUTimer>>,
    poll_send_timer: Mutex<MaybeUninit<qemu_timer_sys::QEMUTimer>>,
    poll_send_callback: Mutex<Option<PollSendCallback>>,
    phantom_pinned: PhantomPinned,
    context: Mutex<Weak<crate::Context>>,
    // Constant offset from simulation time to VM time
    // Set in ::start()
    // Concurrency:
    // - read by network emulation code and the deadline handler
    // - written by Qemu main loop when calling ::start()
    offset: Mutex<Duration>,
    // Previous deadline in global simulation time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    prev_deadline: Mutex<StdDuration>,
    // Next deadline in global simulation time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    next_deadline: Mutex<StdDuration>,
}

// Wrapper struct to avoid conflicts between Pin::new() and TimerContextInner::new()
#[derive(Debug)]
pub struct TimerContext(Pin<Arc<TimerContextInner>>);

impl TimerContext {
    pub(crate) fn new(_config: &crate::Config) -> Result<TimerContext> {
        Ok(TimerContext(Arc::pin(TimerContextInner::new())))
    }
}

impl Deref for TimerContext {
    type Target = Pin<Arc<TimerContextInner>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TimerContextInner {
    fn new() -> TimerContextInner {
        let qemu_timer = Mutex::new(MaybeUninit::uninit());
        let poll_send_timer = Mutex::new(MaybeUninit::uninit());
        let poll_send_callback = Mutex::new(None);
        let phantom_pinned = PhantomPinned;
        let context = Mutex::new(Weak::new());
        let offset = Mutex::new(Duration::zero());
        let prev_deadline = Mutex::new(Default::default());
        let next_deadline = Mutex::new(StdDuration::new(0, 0));

        TimerContextInner {
            qemu_timer,
            poll_send_timer,
            poll_send_callback,
            phantom_pinned,
            context,
            offset,
            prev_deadline,
            next_deadline,
        }
    }

    fn set_next_deadline(self: &Pin<Arc<Self>>, deadline: StdDuration) {
        let mut next_deadline = self.next_deadline.lock().unwrap();
        let next_deadline_val = *next_deadline;
        *self.prev_deadline.lock().unwrap() = next_deadline_val;
        *next_deadline = deadline;

        // Safety:
        // - Qemu clocks are assumed initialized when self is created
        // - qemu_clock_get_ns() only accesses Qemu's internal data
        // - qemu_clock_get_ns() does not require locking
        // let vm_time = unsafe { qemu_clock_get_ns(QEMUClockType::QEMU_CLOCK_VIRTUAL) };
        let timer_deadline = (self.offset.lock().unwrap().to_std().unwrap() + deadline).as_nanos() as i64;
        // Safety:
        // - qemu_timer is pinned
        // - qemu_timer is initialized in ::start() and de-initialized in ::stop()
        // - ::set_next_deadline() is not called between ::stop() and ::start() because:
        //   - the only callers are ::start() and deadline_handler()
        //   - ::stop() -> timer_del() makes sure that deadline_handler() is not called or in
        //     progress before it returns
        // - timer_mod() is thread-safe
        let qemu_timer = self.qemu_timer.lock().unwrap().as_mut_ptr();
        unsafe { qemu_timer_sys::timer_mod(qemu_timer, timer_deadline) };
    }

    pub fn start(self: &Pin<Arc<Self>>, deadline: StdDuration) -> Result<Duration> {
        // TODO: Make sure ::start() is not called again before ::stop()

        // Count a new reference to self in qemu_timer
        // We cannot use Arc::into_raw() to keep the reference after the end of the function so we
        // combine ManuallyDrop and Pin::as_ref().get_ref().
        let opaque = ManuallyDrop::new(self.clone());
        let opaque = opaque.as_ref().get_ref() as *const TimerContextInner as *mut std::os::raw::c_void;
        // Count a new reference to self in poll_send_timer
        let opaque2 = ManuallyDrop::new(self.clone());
        assert_eq!(opaque2.as_ref().get_ref() as *const TimerContextInner as *mut std::os::raw::c_void, opaque);

        // Safety: TODO
        let qemu_timer = self.qemu_timer.lock().unwrap().as_mut_ptr();
        let poll_send_timer = self.poll_send_timer.lock().unwrap().as_mut_ptr();
        unsafe {
            qemu_timer_sys::timer_init_full(qemu_timer,
                std::ptr::null_mut(),
                QEMUClockType::QEMU_CLOCK_VIRTUAL,
                qemu_timer_sys::SCALE_NS,
                0,
                Some(deadline_handler),
                opaque);
            qemu_timer_sys::timer_init_full(poll_send_timer,
                std::ptr::null_mut(),
                QEMUClockType::QEMU_CLOCK_VIRTUAL,
                qemu_timer_sys::SCALE_NS,
                0,
                Some(poll_send_handler),
                opaque);
        }

        // Safety:
        // - Qemu clocks are assumed initialized when self is created
        // - qemu_clock_get_ns() only accesses Qemu's internal data
        // - qemu_clock_get_ns() does not require locking
        let vm_time = unsafe { qemu_clock_get_ns(QEMUClockType::QEMU_CLOCK_VIRTUAL) };
        let vm_time = Duration::nanoseconds(vm_time);
        *self.offset.lock().unwrap() = vm_time;

        self.set_next_deadline(deadline);

        Ok(vm_time)
    }

    // TODO: Currently unsafe! Assumes that start() has been called before and that stop() is never
    // called twice. Otherwise calling stop() prematurately drops self!
    pub fn stop(self: &Pin<Arc<Self>>) {
        // Safety: TODO
        let qemu_timer = self.qemu_timer.lock().unwrap().as_mut_ptr();
        let poll_send_timer = self.poll_send_timer.lock().unwrap().as_mut_ptr();
        unsafe {
            qemu_timer_sys::timer_del(qemu_timer);
            qemu_timer_sys::timer_deinit(qemu_timer);
            qemu_timer_sys::timer_del(poll_send_timer);
            qemu_timer_sys::timer_deinit(poll_send_timer);
        }
        // Drop the reference given to qemu_timer
        // It is easier to use the opaque pointer from self than using the one from qemu_timer
        let ptr = self.deref() as *const TimerContextInner;
        // Safety: Arc::from_raw() gets back the ManuallyDrop'ed reference given by Arc::clone() in
        // ::start()
        unsafe {
            drop(Arc::from_raw(ptr));
            drop(Arc::from_raw(ptr));
        }
    }

    /// Returns the application local time adjusted to compensate simulation delays
    pub fn application_now(&self) -> chrono::NaiveDateTime {
        // TODO: Better feature-gate gettimeofday() and friends
        unimplemented!()
    }

    /// Returns the global simulation time
    pub fn simulation_now(&self) -> StdDuration {
        // Safety:
        // - Qemu clocks are assumed initialized when self is created
        // - qemu_clock_get_ns() only accesses Qemu's internal data
        // - qemu_clock_get_ns() does not require locking
        let vm_time = unsafe { qemu_clock_get_ns(QEMUClockType::QEMU_CLOCK_VIRTUAL) };
        // This is a bug if vm_time is lower than offset, so the conversion to StdDuration cannot
        // fail.
        (Duration::nanoseconds(vm_time) - *self.offset.lock().unwrap()).to_std().unwrap()
    }

    pub fn simulation_previous_deadline(&self) -> StdDuration {
        *self.prev_deadline.lock().unwrap()
    }

    pub fn simulation_next_deadline(&self) -> StdDuration {
        *self.next_deadline.lock().unwrap()
    }

    pub fn check_deadline_overrun(&self, send_time: StdDuration, mut _upcoming_messages: &Mutex<VecDeque<OutputMsg>>) -> Option<StdDuration> {
        if send_time > self.simulation_next_deadline() {
            // Contrary to the KVM case, we control whether packets are timestamped out-of-order
            // across deadlines or not. No need to fix the ordering here.
            Some(send_time)
        } else {
            None
        }
    }

    pub fn poll_send_latency(&self) -> StdDuration {
        StdDuration::ZERO
    }

    pub fn schedule_poll_send_callback(&self, now: StdDuration, later: Option<StdDuration>, callback: &Arc<crate::PollSendCallback>) {
        // In icount mode rescheduling exactly now may start an infinite loop
        let later = later.unwrap_or(now + StdDuration::from_nanos(1));
        let expire = (self.offset.lock().unwrap().to_std().unwrap() + later).as_nanos() as i64;

        // Safety: similar arguments to qemu_timer in ::set_next_deadline
        let poll_send_timer = self.poll_send_timer.lock().unwrap().as_mut_ptr();
        *self.poll_send_callback.lock().unwrap() = Some(PollSendCallback(callback.clone()));
        unsafe { qemu_timer_sys::timer_mod(poll_send_timer, expire) };
    }
}

pub fn register(context: &Arc<crate::Context>) -> Result<()> {
    let timer_context = &context.timer_context;
    *timer_context.context.lock().unwrap() = Arc::downgrade(context);
    Ok(())
}

extern "C" fn deadline_handler(opaque: *mut ::std::os::raw::c_void) {
    use crate::AfterDeadline;

    // Safety: TODO
    let timer_context = unsafe { (opaque as *const TimerContextInner).as_ref().unwrap() };
    if let Some(context) = timer_context.context.lock().unwrap().upgrade() {
        match context.at_deadline() {
            AfterDeadline::NextDeadline(deadline) => {
                context.timer_context.set_next_deadline(deadline);
            },
            AfterDeadline::EndSimulation => (),
        }
    }
}

extern "C" fn poll_send_handler(opaque: *mut ::std::os::raw::c_void) {
    // Safety: TODO
    let timer_context = unsafe { (opaque as *const TimerContextInner).as_ref().unwrap() };
    if let Some(ref callback) = *timer_context.poll_send_callback.lock().unwrap() {
        callback.0();
    }
}
