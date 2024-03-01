use chrono::Duration;
use libc::{c_int, mmap, PROT_READ, MAP_SHARED};
use qemu_timer_sys::QEMUClockType;
use std::collections::VecDeque;
use std::convert::TryFrom;
use std::fs;
use std::io::Result;
use std::marker::PhantomPinned;
use std::mem::ManuallyDrop;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::pin::Pin;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration as StdDuration;

use crate::output_msg_set::{OutputMsg};

use core::arch::x86_64::{_rdtsc};

use log::{debug, warn};

extern {
    fn open_device() -> c_int;
    fn close_device(fd: c_int);
    fn ioctl_register_deadline(fd: c_int, deadline: u64, deadline_tsc: u64) -> u64;
    fn ioctl_init_check(fd: c_int) -> bool;
}

#[repr(transparent)]
struct PollSendCallback(Arc<crate::PollSendCallback>);

impl std::fmt::Debug for PollSendCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("PollSendCallback: {:?}", Arc::as_ptr(&self.0)))
    }
}

#[repr(C)]
struct TimerTSCInfos {
    tsc_offset: u64,
    tsc_scaling_ratio: u64,
}

// Used with all values in nanoseconds but the code only requires that all values use the same
// units
#[derive(Debug)]
struct PollSendLatencyEstimator {
    estimate: i32,
    last_scheduled: i64,
    delta: i32,
    recent_sum: f64,
    num_recent_samples: u32,
    old_sum: f64,
    num_old_samples: u32,
}

// Adapted from KVM lapic timer latency estimator
impl PollSendLatencyEstimator {
    // around 10µs were observed
    const INIT: i32 = 10_000;
    const NUM_MAX: u32 = 1_000_000;

    fn new() -> PollSendLatencyEstimator {
        PollSendLatencyEstimator {
            estimate: Self::INIT,
            last_scheduled: 0,
            delta: 0,
            recent_sum: 0.0,
            num_recent_samples: 0,
            old_sum: 0.0,
            num_old_samples: 0,
        }
    }

    fn set_next_scheduled(&mut self, timestamp: i64) {
        assert!(timestamp >= 0);
        self.last_scheduled = timestamp;
        self.delta = 0;
    }

    fn new_record(&mut self, timestamp: i64) {
        assert!(timestamp >= 0);
        let delta = timestamp - self.last_scheduled;
        self.delta = i32::try_from(delta)
            .expect("PollSendLatencyEstimator::new_record: latency above 2s");
    }

    fn adjust(&mut self) {
        if self.delta == 0 {
            return;
        }

        let sample = self.delta + self.estimate;
        if sample <= 0 {
            self.delta = 0;
            return;
        }

        if self.num_recent_samples >= Self::NUM_MAX {
            self.old_sum = self.recent_sum;
            self.num_old_samples = self.num_recent_samples;
            self.recent_sum = 0.0;
            self.num_recent_samples = 0;
        }

        self.recent_sum += 1.0 / f64::from(sample);
        self.num_recent_samples += 1;
        let estimate = (f64::from(self.num_recent_samples + self.num_old_samples) / (self.recent_sum + self.old_sum)).round();
        if estimate >= f64::from(i32::MIN) && estimate <= f64::from(i32::MAX) {
            self.estimate = unsafe { estimate.to_int_unchecked() };
        }

        self.delta = 0;
    }

    // Estimate on-demand to avoid adding latency at timer expiry
    fn get(&mut self) -> i32 {
        self.adjust();
        self.estimate
    }
}

#[derive(Debug)]
pub struct TimerContextInner {
    poll_send_timer: Mutex<MaybeUninit<qemu_timer_sys::QEMUTimer>>,
    poll_send_callback: Mutex<Option<PollSendCallback>>,
    poll_send_latency: Mutex<PollSendLatencyEstimator>,
    phantom_pinned: PhantomPinned,
    context: Mutex<Weak<crate::Context>>,
    // Previous deadline in global simulation time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    prev_deadline: Mutex<StdDuration>,
    // Next deadline in global simulation time
    // No concurrency: (mut) accessed only by the deadline handler
    // Mutex is used to show interior mutability despite sharing.
    next_deadline: Mutex<StdDuration>,
    guest_tsc : Mutex<u64>, // Value of the guest tsc register at the beginning of slot before the last deadline handled
    vmx_timer_value : Mutex<u64>, // Value of the deadline used to setup the VMX Preemption timer. It's the equivalent of next_deadline at the scale of the guest
    tsc_freq : Mutex<f64>, // frequency of guest TSC in GHz,
    fd: Mutex<c_int>, // file descriptor of the kernel module
    tsc_infos: *mut TimerTSCInfos,
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
        let poll_send_timer = Mutex::new(MaybeUninit::uninit());
        let poll_send_callback = Mutex::new(None);
        let poll_send_latency = Mutex::new(PollSendLatencyEstimator::new());
        let phantom_pinned = PhantomPinned;
        let context = Mutex::new(Weak::new());
        let prev_deadline = Mutex::new(Default::default());
        let next_deadline = Mutex::new(StdDuration::new(0, 0));
        let guest_tsc = Mutex::new(0);
        let vmx_timer_value = Mutex::new(0);
        let tsc_freq = Mutex::new(0.0);

        unsafe {
            let fd_c_int = open_device();
            let fd = Mutex::new(fd_c_int);

            let tsc_infos = mmap(null_mut(), 4096, PROT_READ, MAP_SHARED, fd_c_int, 0) as *mut TimerTSCInfos;

            TimerContextInner {
                poll_send_timer,
                poll_send_callback,
                poll_send_latency,
                phantom_pinned,
                context,
                prev_deadline,
                next_deadline,
                guest_tsc,
                vmx_timer_value,
                tsc_freq,
                fd,
                tsc_infos
            }
        }
    }

    pub fn set_next_deadline(self: &Pin<Arc<Self>>, deadline: StdDuration) {
        let next_deadline_val = *self.next_deadline.lock().unwrap();
       
        let timer_deadline = (deadline - next_deadline_val).as_nanos() as u64;
        *self.prev_deadline.lock().unwrap() = next_deadline_val;
        *self.next_deadline.lock().unwrap() = deadline;

        let timer_deadline_tsc = timer_deadline as f64 * *self.tsc_freq.lock().unwrap();
        // ioctls are unsafe 
        unsafe {
            // Send timer_deadline to the tansiv-timer kernel module
            let fd = *self.fd.lock().unwrap();
            let vmx_timer_value =  ioctl_register_deadline(fd, timer_deadline, timer_deadline_tsc as u64);
            *self.vmx_timer_value.lock().unwrap() = vmx_timer_value;
        };
    }

    pub fn start(self: &Pin<Arc<Self>>, deadline: StdDuration) -> Result<Duration> {
        // TODO: Make sure ::start() is not called again before ::stop()

        // Check if the initialization of the kernel module has been done
        // ioctls are unsafe
        unsafe {
            let fd = *self.fd.lock().unwrap();
            let init_done : bool = ioctl_init_check(fd);
            if !init_done {
                panic!("Kernel module is not correctly initialized!");
            }
        }
        
        // First deadline : read tsc frequency from sysfs and convert it to GHz
        let mut tsc_freq : f64 = 1.0;
        match fs::read_to_string("/sys/devices/system/cpu/tsc_khz") {
            Err(why) => debug!("Failed to read tsc frequency from sysfs: {:?}", why),
            Ok(tsc_khz_str) => match tsc_khz_str.trim().parse::<i64>() {
                Err(why) => debug!("Failed to convert tsc frequency to GHz: {:?}", why),
                Ok(tsc_khz_int) => tsc_freq = (tsc_khz_int as f64) / 1000000.0,
            }
        }

        // Count a new reference to self in poll_send_timer
        // We cannot use Arc::into_raw() to keep the reference after the end of the function so we
        // combine ManuallyDrop and Pin::as_ref().get_ref().
        let opaque = ManuallyDrop::new(self.clone());
        let opaque = opaque.as_ref().get_ref() as *const TimerContextInner as *mut std::os::raw::c_void;
        // Safety: TODO
        let poll_send_timer = self.poll_send_timer.lock().unwrap().as_mut_ptr();
        unsafe {
            qemu_timer_sys::timer_init_full(poll_send_timer,
                std::ptr::null_mut(),
                QEMUClockType::QEMU_CLOCK_VIRTUAL,
                qemu_timer_sys::SCALE_NS,
                0,
                Some(poll_send_handler),
                opaque);
        }

        *self.tsc_freq.lock().unwrap() = tsc_freq;
        self.set_next_deadline(deadline);

        Ok(Duration::zero())
    }

    // TODO: Currently unsafe! Assumes that start() has been called before and that stop() is never
    // called twice. Otherwise calling stop() prematurately drops self!
    pub fn stop(self: &Pin<Arc<Self>>) {
        // Safety: TODO
        let poll_send_timer = self.poll_send_timer.lock().unwrap().as_mut_ptr();
        unsafe {
            qemu_timer_sys::timer_del(poll_send_timer);
            qemu_timer_sys::timer_deinit(poll_send_timer);
        }
        // Drop the reference given to poll_send_timer
        // It is easier to use the opaque pointer from self than using the one from poll_send_timer
        let ptr = self.deref() as *const TimerContextInner;
        // Safety: Arc::from_raw() gets back the ManuallyDrop'ed reference given by Arc::clone() in
        // ::start()
        unsafe {
            close_device(*self.fd.lock().unwrap());
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
        unsafe{
            // Get current timestamp
            let now = _rdtsc();
            // Convert it to guest tsc scale
            let now_guest = (now + (*self.tsc_infos).tsc_offset) as f64;
            // Get the value of the deadline in guest tsc scale
            let next_deadline_guest = *self.vmx_timer_value.lock().unwrap() as f64;
            // Check that the timestamp is not greater than the deadline. In
            // this case, log it.
            if now_guest > next_deadline_guest {
                debug!("Message time stamped {} virtual TSC ticks after the deadline.\n", (now_guest - next_deadline_guest) as i64);
            }
            // Get prev and next deadline in ns
            let next_deadline = self.next_deadline.lock().unwrap().as_nanos() as u64;
            // Get tsc freq
            let tsc_freq = *self.tsc_freq.lock().unwrap();
            // We have Simulation_time = (Guest_TSC / TSC_freq) - Offset
            // There is an offset because the VMs start their clock before the
            // synchronisation begins with the call to vsg_start
            // Thanks to next_deadline_guest and next_deadline, we can compute
            // this offset
            let offset = (next_deadline_guest / tsc_freq) as u64 - next_deadline;
            let vm_time = (now_guest / tsc_freq) as u64 - offset;
            match Duration::nanoseconds(vm_time as i64).to_std()
            {
                Err(_)  => return StdDuration::ZERO, // can happen if a message is sent before vsg_start
                Ok(val) => return val,
            }
        }
    }

    pub fn simulation_previous_deadline(&self) -> StdDuration {
        *self.prev_deadline.lock().unwrap()
    }

    pub fn simulation_next_deadline(&self) -> StdDuration {
        *self.next_deadline.lock().unwrap()
    }

    pub fn check_deadline_overrun(&self, send_time: StdDuration, list: &Mutex<VecDeque<OutputMsg>>) -> Option<StdDuration> {
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

    pub fn poll_send_latency(&self) -> StdDuration {
        let latency = self.poll_send_latency.lock().unwrap().get();
        StdDuration::from_nanos(u64::try_from(latency)
                                .expect("poll_send_latency: negative"))
    }

    pub fn schedule_poll_send_callback(&self, now: StdDuration, later: Option<StdDuration>, callback: &Arc<crate::PollSendCallback>) {
        // Use QEMU_VIRTUAL_CLOCK to schedule the timer and try to be close to the desired scheduled
        // time
        // If a deadline occurs before the timer expires, the timer will appear as expiring before
        // the desired scheduled time. The polling entity may thus poll a bit more frequently than
        // needed but hopefully not too much.

        // Safety:
        // - Qemu clocks are assumed initialized when self is created
        // - qemu_clock_get_ns() only accesses Qemu's internal data
        // - qemu_clock_get_ns() does not require locking
        let qemu_now = unsafe { qemu_timer_sys::qemu_clock_get_ns(QEMUClockType::QEMU_CLOCK_VIRTUAL) };

        let later = later.unwrap_or(now);
        let delay_ns = i64::try_from((later-now).as_nanos())
            .expect("schedule_poll_send_callback: delay_ns as i64 overflow");

        let raw_expire = qemu_now.checked_add(delay_ns)
            .expect("schedule_poll_send_callback: raw expire as i64 overflow");

        let mut latency = self.poll_send_latency.lock().unwrap();
        let latency_ns = i64::from(latency.get());

        latency.set_next_scheduled(raw_expire);
        drop(latency);

        let expire = if latency_ns < delay_ns {
            raw_expire - latency_ns
        } else {
            warn!("schedule_poll_send_callback: latency_ns = {}, delay_ns = {}", latency_ns, delay_ns);
            qemu_now
        };

        // Code that could be used for TSC-based expiry calculation
        //
        // // Safety:
        // // Called with the IO thread locked, either in the vCPU thread or in the IO thread
        // // self.tsc_infos is modified only in the vCPU thread while holding the IO thread lock.
        // //
        // // Although tsc_infos.tsc_offset is u64 it is used as an i64, that is it encodes a negative
        // // value that the CPU adds with an overflow to a host TSC value in order to actually substract
        // // a positive offset.
        // let tsc_offset = unsafe { (*self.tsc_infos).tsc_offset as i64 } as f64;
        // assert!(tsc_offset < 0.0);

        // let offset_ns = -(tsc_offset / *self.tsc_freq.lock().unwrap()) as i64;
        // assert!(offset_ns > 0);
        // let expire = i64::try_from(later.as_nanos())
            // .expect("schedule_poll_send_callback: guest later as i64 overflow")
            // .checked_add(offset_ns)
            // .expect("schedule_poll_send_callback: uncompensated guest later i64 overflow");

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

#[no_mangle]
pub extern "C" fn deadline_handler(opaque: *mut ::std::os::raw::c_void, guest_tsc: u64) -> u64 {
    use crate::AfterDeadline;
    // Safety: TODO
    let context_arg = unsafe { (opaque as *const crate::Context).as_ref().unwrap() };
    let timer_context = &context_arg.timer_context;
    *timer_context.guest_tsc.lock().unwrap() = guest_tsc;
    if let Some(context) = timer_context.context.lock().unwrap().upgrade() {
        match context_arg.at_deadline() {
            AfterDeadline::NextDeadline(deadline) => {
                context.timer_context.set_next_deadline(deadline);

            },
            AfterDeadline::EndSimulation => {
                panic!("Ending simulation, at_deadline_failed!");
            }
        }
    }
    // return timer_context.prev_deadline.lock().unwrap().as_nanos() as u64;
    return timer_context.next_deadline.lock().unwrap().as_nanos() as u64 - timer_context.prev_deadline.lock().unwrap().as_nanos() as u64;
}

#[no_mangle]
pub extern fn get_tansiv_timer_fd(opaque: *mut ::std::os::raw::c_void) -> c_int {
    let context_arg = unsafe { (opaque as *const crate::Context).as_ref().unwrap() };
    let timer_context = &context_arg.timer_context;
    return *timer_context.fd.lock().unwrap();
}

extern "C" fn poll_send_handler(opaque: *mut ::std::os::raw::c_void) {
    // Safety: TODO
    let timer_context = unsafe { (opaque as *const TimerContextInner).as_ref().unwrap() };
    let guard = timer_context.poll_send_callback.lock().unwrap();
    if let Some(ref callback) = *guard {
        // callback can itself call ::schedule_poll_send_callback() and take the lock again
        let callback = callback.0.clone();
        drop(guard);
        // Safety:
        // - Qemu clocks are assumed initialized when self is created
        // - qemu_clock_get_ns() only accesses Qemu's internal data
        // - qemu_clock_get_ns() does not require locking
        let qemu_now = unsafe { qemu_timer_sys::qemu_clock_get_ns(QEMUClockType::QEMU_CLOCK_VIRTUAL) };
        timer_context.poll_send_latency.lock().unwrap().new_record(qemu_now);
        callback();
    }
}
