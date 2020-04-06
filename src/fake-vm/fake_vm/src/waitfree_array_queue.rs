use crossbeam_queue::{ArrayQueue, PushError};
use std::fmt;
use std::marker::PhantomData;

#[derive(Debug)]
pub enum Error<I: std::fmt::Debug> {
    NoSlotAvailable {
        item: I,
    },
}

type Result<T, I> = std::result::Result<T, Error<I>>;

impl<I: std::fmt::Debug> fmt::Display for Error<I> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self {
            Error::NoSlotAvailable { item: _, } => "No slot available",
        };
        write!(f, "{}", msg)
    }
}

impl<I: std::fmt::Debug> std::error::Error for Error<I> {}

// Multiple-producers multiple consumer lock-less FIFO queue
// Is waitfree when used as an SPSC queue.
#[derive(Debug)]
pub struct WaitfreeArrayQueue<I> {
    queue: ArrayQueue<I>,
}

pub struct WaitfreeArrayQueueIter<'a, I> {
    queue: &'a WaitfreeArrayQueue<I>,
    phantom_data: PhantomData<&'a I>,
}

impl<'a, I: std::fmt::Debug> Iterator for WaitfreeArrayQueueIter<'a, I> {
    type Item = I;

    fn next<'b>(&'b mut self) -> Option<I> {
        self.queue.pop()
    }
}

impl<I: std::fmt::Debug> WaitfreeArrayQueue<I> {
    pub fn new(num_slots: usize) -> WaitfreeArrayQueue<I> {
        let queue = ArrayQueue::new(num_slots);

        WaitfreeArrayQueue { queue: queue, }
    }

    pub fn push(&self, item: I) -> Result<(), I> {
        match self.queue.push(item) {
            Ok(_) => Ok(()),
            Err(PushError(item)) => Err(Error::NoSlotAvailable { item: item, }),
        }
    }

    pub fn pop(&self) -> Option<I> {
        self.queue.pop().ok()
    }

    pub fn iter<'a>(&'a self) -> WaitfreeArrayQueueIter<'a, I> {
        WaitfreeArrayQueueIter {
            queue: self,
            phantom_data: PhantomData,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
