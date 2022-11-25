use chrono::Duration;
use libc::{getpid};
use std::io::Result;
use std::marker::PhantomPinned;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration as StdDuration;


// log_write
use std::fs;
use std::io::Write;
use core::arch::x86_64::{_rdtsc};

extern {
    fn ioctl_register_deadline(pid: i32, deadline: u64, deadline_tsc: u64, vmenter_guest_tsc: u64) -> u64;
    fn ioctl_init_check(pid: i32) -> bool;
    fn ioctl_scale_tsc(pid: i32, tsc: u64) -> i64;
}


#[derive(Debug)]
pub struct TimerContextInner {
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
    guest_tsc : Mutex<u64>, // Value of the guest tsc register at the beginning of slot before the last deadline handled
    vmx_timer_value : Mutex<u64>, // Value of the deadline used to setup the VMX Preemption timer. It's the equivalent of next_deadline at the scale of the guest
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

fn log_write(message: &str) {
    unsafe {
    let filename = format!("/tmp/tansiv_rust_{:?}.csv", getpid());
    let mut f = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(filename)
        .expect("Unable to open the file");
    f.write_all(message.as_bytes()).expect("Unable to write to the file");
    }
}

impl TimerContextInner {
    fn new() -> TimerContextInner {
        let phantom_pinned = PhantomPinned;
        let context = Mutex::new(Weak::new());
        let offset = Mutex::new(Duration::zero());
        let prev_deadline = Mutex::new(Default::default());
        let next_deadline = Mutex::new(StdDuration::new(0, 0));
        let guest_tsc = Mutex::new(0);
        let vmx_timer_value = Mutex::new(0);

        TimerContextInner {
            phantom_pinned,
            context,
            offset,
            prev_deadline,
            next_deadline,
            guest_tsc,
            vmx_timer_value
        }
    }

    pub fn set_next_deadline(self: &Pin<Arc<Self>>, deadline: StdDuration, first_deadline: bool) {
        static mut INIT_DONE: bool = false;
        let mut next_deadline = self.next_deadline.lock().unwrap();
        let next_deadline_val = *next_deadline;

        let mut timer_deadline = (deadline - next_deadline_val).as_nanos() as u64;
        *self.prev_deadline.lock().unwrap() = next_deadline_val;
        *next_deadline = deadline;
        // Send timer_deadline to the tansiv-timer kernel module
        unsafe {
            while !(INIT_DONE) {
                INIT_DONE = ioctl_init_check(getpid());
            }
            // TODO : Find something to get the tsc freq
            if (first_deadline) {
                timer_deadline = 60 * 1000000000; // Do nothing for 1 min
            }
            let mut timer_deadline_tsc = timer_deadline as f64;
            timer_deadline_tsc *= 2.8032;
            let vmenter_guest_tsc = *self.guest_tsc.lock().unwrap();
            let vmx_timer_value =  ioctl_register_deadline(getpid(), timer_deadline, timer_deadline_tsc as u64, vmenter_guest_tsc);
            *self.vmx_timer_value.lock().unwrap() = vmx_timer_value;
        };
    }

    pub fn start(self: &Pin<Arc<Self>>, deadline: StdDuration) -> Result<Duration> {
        // TODO: Make sure ::start() is not called again before ::stop()

        self.set_next_deadline(deadline, true);

        Ok(Duration::zero())
    }

    // TODO: Currently unsafe! Assumes that start() has been called before and that stop() is never
    // called twice. Otherwise calling stop() prematurately drops self!
    pub fn stop(self: &Pin<Arc<Self>>) {
        // Safety: TODO
        // TODO : Add stuff to clean tansiv_module
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
        unimplemented!()
    }

    /// Returns the global simulation time
    pub fn simulation_now(&self) -> StdDuration {
        // let vm_time = unsafe
        // {qemu_clock_get_ns(QEMUClockType::QEMU_CLOCK_VIRTUAL)};
        // ioctls involved so everythng is unsafe
        unsafe{
            // Get current timestamp
            let now = _rdtsc();
            // let pipe_name = CString::new("/tmp/tansiv-fifo").expect("CString::new failed");
            // let fd = open(pipe_name.as_ptr(), O_RDONLY | O_NONBLOCK);
            // if fd < 0 {
                // panic!("simulation_now: failed to open tansiv-fifo!");
            // }
            // let buf : &mut [u8; 8] = &mut [0, 0, 0, 0, 0, 0, 0, 0];
            // let err = read(fd, buf.as_ptr() as *mut c_void, 8);
            // if err != 8 {
                // panic!("simulation_now: failed to read tansiv-fifo! err is {}", err);
            // }
            // close(fd);
            // let now = u64::from_le_bytes(*buf);
            
            // Convert it to guest tsc scale
            let mut now_guest = ioctl_scale_tsc(getpid(), now) as f64;
            // Get the value of the deadline in guest tsc scale
            let next_deadline_guest = *self.vmx_timer_value.lock().unwrap() as f64;
            // Check that the timestamp is not greater than the deadline. In
            // this case, adjust it to the value of the deadline
            // assert!(now_guest <= next_deadline_guest, "now_guest : {} ; next_deadline_guest : {} \n", now_guest, next_deadline_guest);
            if now_guest > next_deadline_guest {
                log_write(&format!("Message time stamped {} virtual TSC ticks after the deadline.\n", (now_guest - next_deadline_guest) as i64));
                now_guest = next_deadline_guest;

            }
            // Get prev and next deadline in ns
            let next_deadline = self.next_deadline.lock().unwrap().as_nanos() as u64;
            let prev_deadline = self.prev_deadline.lock().unwrap().as_nanos() as u64;
            // Get the duration of the current deadline slot
            let slot_duration_ns =  (next_deadline - prev_deadline) as f64; 
            // Get the duration of the slot in tsc ticks
            let slot_duration_tsc = (slot_duration_ns as f64) * 2.8032;
            // Compute the date of the timestamp
            let vm_time = prev_deadline + ((1.0 - (next_deadline_guest - now_guest) / slot_duration_tsc) * slot_duration_ns) as u64;
            return Duration::nanoseconds(vm_time as i64).to_std().unwrap();
        }
    }

    pub fn simulation_previous_deadline(&self) -> StdDuration {
        *self.prev_deadline.lock().unwrap()
    }

    pub fn simulation_next_deadline(&self) -> StdDuration {
        *self.next_deadline.lock().unwrap()
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
                context.timer_context.set_next_deadline(deadline, false);

            },
            AfterDeadline::EndSimulation => {
                panic!("Ending simulation, at_deadline_failed!");
            }
        }
    }
    // return timer_context.prev_deadline.lock().unwrap().as_nanos() as u64;
    return timer_context.next_deadline.lock().unwrap().as_nanos() as u64 - timer_context.prev_deadline.lock().unwrap().as_nanos() as u64;
}
