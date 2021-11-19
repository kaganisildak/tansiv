#![no_std]

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering, fence, spin_loop_hint};

#[derive(Debug)]
pub struct SeqLock<T> {
    value: UnsafeCell<T>,
    seq_count: AtomicUsize,
}

// Make sure that no wrapped value is dropped with side effects in read() or write() by
// restricting to Copy types
impl<T: Copy> SeqLock<T> {
    pub fn new(value: T) -> SeqLock<T> {
        SeqLock {
            value: UnsafeCell::new(value),
            seq_count: AtomicUsize::new(0),
        }
    }

    fn wait_released(&self) -> usize {
        loop {
            let count = self.seq_count.load(Ordering::Acquire);
            if count & 1 == 0 {
                return count;
            }
            spin_loop_hint();
        }
    }

    fn check_concurrent_write(&self, prev_count: usize) -> Option<()> {
        assert_eq!(prev_count & 1, 0);

        // Ensure that all previous accesses since ::wait_released() happen before the load()
        // below. A load(Ordering::Release) would have done the job, if it existed.
        fence(Ordering::AcqRel);

        let count = self.seq_count.load(Ordering::Relaxed);
        if prev_count == count {
            None
        } else {
            Some(())
        }
    }

    pub fn read<U, F: Fn(T) -> U>(&self, read_fn: F) -> U {
        loop {
            let count = self.wait_released();

            // read_volatile() to make sure that no UB happens because of a changing value in case
            // of a concurrent write
            // TODO: Is such UB possible with T: Copy ?
            let value = unsafe { self.value.get().read_volatile() };
            let ret = read_fn(value);

            if self.check_concurrent_write(count).is_none() {
                return ret;
            }
        }
    }

    pub fn write<F: FnOnce(T) -> T>(&self, write_fn: F) {
        let count = (|| loop {
            let count = self.wait_released();
            let enter_count = self.seq_count.compare_and_swap(count, count + 1, Ordering::Acquire);
            if enter_count == count {
                return count;
            }
        })();

        let inner = self.value.get();
        unsafe {
            let value = inner.read();
            let value = write_fn(value);
            inner.write(value);
        }

        let exit_count = self.seq_count.fetch_add(1, Ordering::Release);
        assert_eq!(exit_count & 1, 1);
        assert_eq!(count + 1, exit_count);
    }
}

unsafe impl<T: Copy> Send for SeqLock<T> {}
unsafe impl<T: Copy> Sync for SeqLock<T> {}
