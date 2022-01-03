use self::Error::*;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Debug)]
pub enum Error {
    NoBufferAvailable,
    SizeTooBig,
}

type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self {
            NoBufferAvailable => "No buffer available",
            SizeTooBig => "Size too big",
        };
        write!(f, "{}", msg)
    }
}

impl std::error::Error for Error {}

#[derive(Debug)]
pub struct InnerBufferPool<T: InnerBuffer> {
    buffer_size: usize,
    buffers: T::Array,
    buffer_busy: Vec<AtomicBool>,
}

impl<T: InnerBuffer> InnerBufferPool<T> {
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    pub fn buffers(&self) -> &T::Array {
        &self.buffers
    }
}

// The safety of the InnerBuffer trait relies on BufferPool::inner being private
#[derive(Debug)]
pub struct BufferPool<T: InnerBuffer> {
    inner: Arc<InnerBufferPool<T>>
}

impl<T: InnerBuffer> Clone for BufferPool<T> {
    fn clone(&self) -> Self {
        BufferPool {
            inner: self.inner.clone()
        }
    }
}

impl<T: InnerBuffer> BufferPool<T> {
    pub fn new(buffer_size: usize, num_buffers: usize) -> BufferPool<T> {
        let buffers = T::calloc(buffer_size, num_buffers);
        let mut buffer_busy: Vec<AtomicBool> = Vec::with_capacity(num_buffers);
        buffer_busy.resize_with(num_buffers, Default::default);

        BufferPool {
            inner: Arc::new(InnerBufferPool {
                buffer_size,
                buffers,
                buffer_busy,
            })
        }
    }

    pub fn allocate_buffer(&self, size: usize) -> Result<Buffer<T>> {
        let pool = &self.inner;
        if size <= pool.buffer_size {
            for (idx, slot) in pool.buffer_busy.iter().enumerate() {
                if !slot.swap(true, Ordering::AcqRel) {
                    // TODO: Zero fill
                    return Ok(Buffer {
                        pool: (*self).clone(),
                        index: idx,
                        inner: T::new(idx, size),
                    });
                }
            }
            Err(NoBufferAvailable)
        } else {
            Err(SizeTooBig)
        }
    }

    // Safety:
    // - called only from Buffer<T>::drop
    fn free_buffer(&self, index: usize, _buffer: &mut T) {
        self.inner.buffer_busy[index].store(false, Ordering::Release);
    }
}

unsafe impl<T: InnerBuffer> Send for BufferPool<T> {}
unsafe impl<T: InnerBuffer> Sync for BufferPool<T> {}

pub trait InnerBuffer: Sized {
    type Array: fmt::Debug;

    fn calloc(buffer_size: usize, num_buffers: usize) -> Self::Array;
    fn new(index: usize, size: usize) -> Self;

    fn as_slice<'a, 'b, 'c>(&'a self, pool: &'b InnerBufferPool<Self>, index: usize) -> &'c [u8]
        where 'a: 'c, 'b: 'c;
    fn as_mut_slice<'a, 'b, 'c>(&'a mut self, pool: &'b InnerBufferPool<Self>, index: usize) -> &'c mut [u8]
        where 'a: 'c, 'b: 'c;
}

// Does not implement Clone. It would be unsafe since cloning would mean allocating the buffer
// space twice.
#[derive(Debug)]
pub struct Buffer<T: InnerBuffer> {
    pool: BufferPool<T>,
    index: usize,
    inner: T,
}

impl<T: InnerBuffer> fmt::Display for Buffer<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let bytes = self.deref();
        write!(f, "{:?} / \"", bytes)?;
        for b in bytes {
            write!(f, "{}", char::from(*b))?;
        }
        write!(f, "\"")
    }
}

impl<T: InnerBuffer> Drop for Buffer<T> {
    fn drop(&mut self) {
        let pool = &self.pool;
        let inner = &mut self.inner;
        pool.free_buffer(self.index, inner);
    }
}

impl<T: InnerBuffer> Deref for Buffer<T> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.inner.as_slice(&self.pool.inner, self.index)
    }
}

impl<T: InnerBuffer> DerefMut for Buffer<T> {
    fn deref_mut(&mut self) -> &mut [u8] {
        let pool = &self.pool.inner;
        let inner = &mut self.inner;
        inner.as_mut_slice(pool, self.index)
    }
}
