///! Missing definitions from libc as well as from nix
use std::time::Duration;
use libc::{clockid_t, timespec};

mod sys {
    use libc::{c_int, clockid_t, itimerspec, sigevent};

    #[allow(non_camel_case_types)]
    pub type timer_t = *mut libc::c_void;

    extern "C" {
        pub fn timer_create(
            __clock_id: clockid_t,
            __evp: *mut sigevent,
            __timerid: *mut timer_t
        ) -> c_int;
        pub fn timer_delete(__timerid: timer_t) -> c_int;

        pub fn timer_settime(
            __timerid: timer_t,
            __flags: c_int,
            __value: *const itimerspec,
            __ovalue: *mut itimerspec
        ) -> c_int;
    }
}

// Does it help to try matching the same constants as in libc?
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub enum ClockId {
    Realtime,
    RealtimeCoarse,
    Monotonic,
    MonotonicCoarse,
    MonotonicRaw,
    ProcessCputimeId,
    ThreadCputimeId,
    Boottime,
    RealtimeAlarm,
    BoottimeAlarm,
}

impl Into<clockid_t> for ClockId {
    fn into(self) -> clockid_t {
        match self {
            ClockId::Realtime => libc::CLOCK_REALTIME,
            ClockId::RealtimeCoarse => libc::CLOCK_REALTIME_COARSE,
            ClockId::Monotonic => libc::CLOCK_MONOTONIC,
            ClockId::MonotonicCoarse => libc::CLOCK_MONOTONIC_COARSE,
            ClockId::MonotonicRaw => libc::CLOCK_MONOTONIC_RAW,
            ClockId::ProcessCputimeId => libc::CLOCK_PROCESS_CPUTIME_ID,
            ClockId::ThreadCputimeId => libc::CLOCK_THREAD_CPUTIME_ID,
            ClockId::Boottime => libc::CLOCK_BOOTTIME,
            ClockId::RealtimeAlarm => libc::CLOCK_REALTIME_ALARM,
            ClockId::BoottimeAlarm => libc::CLOCK_BOOTTIME_ALARM,
        }
    }
}

fn duration_to_timespec(duration: Duration) -> timespec {
    timespec {
        tv_sec: duration.as_secs() as libc::time_t,
        tv_nsec: duration.subsec_nanos() as libc::c_long,
    }
}

fn timespec_to_duration(ts: timespec) -> Duration {
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

fn timespec_zero() -> timespec {
    libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    }
}

pub mod clock {
    use super::*;
    use std::io::Result;
    use std::time::Duration;

    pub fn gettime(clock_id: ClockId) -> Result<Duration> {
        let mut time = timespec_zero();
        let res = unsafe { libc::clock_gettime(clock_id.into(), &mut time) };
        if res == 0 {
            Ok(timespec_to_duration(time))
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}

pub mod timer {
    use libc::{c_int, itimerspec};
    use nix::sys::signal::{SigEvent, SigevNotify};
    use std::io::Result;
    use std::time::Duration;
    use super::*;
    use sys::timer_t;

    #[derive(Clone, Copy, Debug)]
    pub struct TimerId {
        inner: timer_t,
    }

    unsafe impl Send for TimerId {}
    unsafe impl Sync for TimerId {}

    impl Default for TimerId {
        fn default() -> TimerId {
            TimerId { inner: std::ptr::null_mut(), }
        }
    }

    impl From<TimerId> for timer_t {
        fn from(timerid: TimerId) -> timer_t {
            timerid.inner
        }
    }

    pub enum SettimeFlags {
        RelativeTime,
        AbsoluteTime,
    }

    impl SettimeFlags {
        fn into_raw(self) -> c_int {
            match self {
                SettimeFlags::RelativeTime => 0,
                SettimeFlags::AbsoluteTime => libc::TIMER_ABSTIME,
            }
        }
    }

    pub fn create(clock_id: ClockId, sigev_notify: Option<SigevNotify>) -> Result<TimerId> {
        let mut sigev = if let Some(notify) = sigev_notify {
            SigEvent::new(notify).sigevent()
        } else {
            // Unused but here to make sure that sigev has a valid address
            SigEvent::new(SigevNotify::SigevNone).sigevent()
        };
        let evp: *mut libc::sigevent = if sigev_notify.is_some() {
            &mut sigev
        } else {
            std::ptr::null_mut()
        };

        unsafe {
            let mut timerid: timer_t = TimerId::default().into();
            let res = sys::timer_create(clock_id.into(), evp, &mut timerid);
            if res == 0 {
                Ok(TimerId { inner: timerid, })
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
    }

    pub fn delete(timer_id: TimerId) {
        unsafe { sys::timer_delete(timer_id.inner); }
    }

    pub fn settime(timer_id: TimerId, flags: SettimeFlags, new_interval: Option<Duration>, new_value: Duration) -> Result<(Duration, Duration)> {
        let new_value = itimerspec {
            it_interval: match new_interval {
                Some(interval) => duration_to_timespec(interval),
                None => timespec_zero(),
            },
            it_value: duration_to_timespec(new_value),
        };
        unsafe {
            let mut old_value: itimerspec = itimerspec {
                it_interval: timespec_zero(),
                it_value: timespec_zero(),
            };
            let res = sys::timer_settime(timer_id.into(), flags.into_raw(), &new_value, &mut old_value);
            if res == 0 {
                Ok((
                        timespec_to_duration(old_value.it_interval),
                        timespec_to_duration(old_value.it_value)
                ))
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
    }
}
