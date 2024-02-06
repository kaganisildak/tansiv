use chrono::Duration;
use libc::c_int;
use std::collections::LinkedList;
use std::fs;
use std::io::Result;
use std::ops::Deref;
use std::pin::Pin;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration as StdDuration;

use crate::output_msg_set::{OutputMsg};

use core::arch::x86_64::{_rdtsc};

use log::debug;

extern {
    fn open_device() -> c_int;
    fn close_device(fd: c_int);
}

#[repr(C)]
struct TimerTSCInfos {
    tsc_offset: u64,
    tsc_scaling_ratio: u64,
    tsc_simulation_offset: u64
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
    guest_tsc : Mutex<u64>, // Value of the guest tsc register at the beginning of slot before the last deadline handled
    tsc_freq : Mutex<f64>, // frequency of guest TSC in GHz,
    fd: Mutex<c_int>, // file descriptor of the kernel module
    tsc_infos: Mutex<*mut TimerTSCInfos>,
    first_deadline : Mutex<u64>, // TSC date of the first deadline
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
        let context = Mutex::new(Weak::new());
        let prev_deadline = Mutex::new(StdDuration::new(0, 0));
        let next_deadline = Mutex::new(StdDuration::new(0, 0));
        let guest_tsc = Mutex::new(0);
        let tsc_freq = Mutex::new(0.0);
        let first_deadline = Mutex::new(0);

        unsafe {
            let fd_c_int = open_device();
            let fd = Mutex::new(fd_c_int);

            let tsc_infos = Mutex::new(null_mut());

            TimerContextInner {
                context,
                prev_deadline,
                next_deadline,
                guest_tsc,
                tsc_freq,
                fd,
                tsc_infos,
                first_deadline
            }
        }
    }

    pub fn set_next_deadline(self: &Pin<Arc<Self>>, deadline: StdDuration) {
        unsafe {
            if !(*self.tsc_infos.lock().unwrap()).is_null() {
                deadline_handler_debug!("Current tsc offset from shared page is {}\n", (*(*self.tsc_infos.lock().unwrap())).tsc_offset);}
            }
        let next_deadline_val = *self.next_deadline.lock().unwrap();
       
        let timer_deadline = (deadline - next_deadline_val).as_nanos() as u64;
        *self.prev_deadline.lock().unwrap() = next_deadline_val;
        *self.next_deadline.lock().unwrap() = deadline;
        // TODO: Used to compute the offset (see simulation_now for detailed comment)
        if *self.first_deadline.lock().unwrap() == 0 {
            *self.first_deadline.lock().unwrap() = (timer_deadline as f64 * *self.tsc_freq.lock().unwrap()) as u64;
        }
    }

    pub fn start(self: &Pin<Arc<Self>>, deadline: StdDuration) -> Result<Duration> {
        // TODO: Make sure ::start() is not called again before ::stop()
        
        // First deadline : read tsc frequency from sysfs and convert it to GHz
        let mut tsc_freq : f64 = 1.0;
        match fs::read_to_string("/sys/devices/system/cpu/tsc_khz") {
            Err(why) => debug!("Failed to read tsc frequency from sysfs: {:?}", why),
            Ok(tsc_khz_str) => match tsc_khz_str.trim().parse::<i64>() {
                Err(why) => debug!("Failed to convert tsc frequency to GHz: {:?}", why),
                Ok(tsc_khz_int) => tsc_freq = (tsc_khz_int as f64) / 1000000.0,
            }
        }
        *self.tsc_freq.lock().unwrap() = tsc_freq;
        self.set_next_deadline(deadline);

        Ok(Duration::zero())
    }

    // TODO: Currently unsafe! Assumes that start() has been called before and that stop() is never
    // called twice. Otherwise calling stop() prematurately drops self!
    pub fn stop(self: &Pin<Arc<Self>>) {
        // Safety: TODO
        // Drop the reference given to qemu_timer
        // It is easier to use the opaque pointer from self than using the one from qemu_timer
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
            // TODO: take into account the TSC scaling ratio
            let now_guest = now + ((*(*self.tsc_infos.lock().unwrap())).tsc_offset as u64);
            // Get tsc freq
            let tsc_freq = *self.tsc_freq.lock().unwrap();
            // We have Simulation_time = ((Guest_TSC - Offset_TSC) / TSC_freq)
            // There is an offset because the VMs start their clock before the
            // synchronisation begins with the call to vsg_start.
            // TODO: There is a second offset to take into account because a
            // deadline is triggered immediately after the vsg_start, and
            // tsc_simulation_offset is initalized at this date. Thus we have
            // subtract the first_deadline from tsc_simulation_offset. Change
            // this as it is confusing and not consistent with the KVM approach
            let offset = (*(*self.tsc_infos.lock().unwrap())).tsc_simulation_offset - *self.first_deadline.lock().unwrap();
            let vm_time = (((now_guest - offset) as f64) / tsc_freq) as u64;
            match Duration::nanoseconds(vm_time as i64).to_std()
            {
                Err(_)  => StdDuration::ZERO, // can happen if a message is sent before vsg_start
                Ok(val) => val,
            }
        }
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
            // that are already enqueued, because the delay of the network card emulation is
            // variable, and of the time adjustments to the VM clock after a
            // deadline.
            // If this happens, change the timestamp of the message to be the
            // same as the last one in the list.
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
    // We return the next time slot duration
    let deadline_duration_nanos: u64 = timer_context.next_deadline.lock().unwrap().as_nanos() as u64 -
                                 timer_context.prev_deadline.lock().unwrap().as_nanos() as u64;
    let deadline_duration_tsc: u64 = (deadline_duration_nanos as f64 * *timer_context.tsc_freq.lock().unwrap()) as u64;
    return deadline_duration_tsc;
}

#[no_mangle]
pub extern "C" fn get_tansiv_timer_fd(opaque: *mut ::std::os::raw::c_void) -> c_int {
    let context_arg = unsafe { (opaque as *const crate::Context).as_ref().unwrap() };
    let timer_context = &context_arg.timer_context;
    return *timer_context.fd.lock().unwrap();
}

#[no_mangle]
pub extern "C" fn set_tansiv_tsc_page(opaque: *mut ::std::os::raw::c_void, memory: *mut ::std::os::raw::c_void) -> c_int {
    let context_arg = unsafe { (opaque as *const crate::Context).as_ref().unwrap() };
    let memory_arg = memory as *mut TimerTSCInfos;
    let timer_context = &context_arg.timer_context;
    *timer_context.tsc_infos.lock().unwrap() = memory_arg;
    return 0;
}