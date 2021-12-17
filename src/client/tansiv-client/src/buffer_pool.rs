use self::Error::*;
use std::cell::UnsafeCell;
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
struct InnerBufferPool {
    buffer_size: usize,
    buffers: UnsafeCell<Vec<u8>>,
    buffer_busy: Vec<AtomicBool>,
}

#[derive(Clone, Debug)]
pub struct BufferPool(Arc<InnerBufferPool>);

impl BufferPool {
    pub fn new(buffer_size: usize, num_buffers: usize) -> BufferPool {
        let mut buffers: Vec<u8> = Vec::with_capacity(buffer_size * num_buffers);
        buffers.resize(buffer_size * num_buffers, 0);
        let mut buffer_busy: Vec<AtomicBool> = Vec::with_capacity(num_buffers);
        buffer_busy.resize_with(num_buffers, Default::default);

        BufferPool(Arc::new(InnerBufferPool {
            buffer_size: buffer_size,
            buffers: UnsafeCell::new(buffers),
            buffer_busy: buffer_busy,
        }))
    }

    pub fn allocate_buffer(&self, size: usize) -> Result<Buffer> {
        let pool = &self.0;
        if size <= pool.buffer_size {
            for (idx, slot) in pool.buffer_busy.iter().enumerate() {
                if !slot.swap(true, Ordering::AcqRel) {
                    // TODO: Zero fill
                    return Ok(Buffer {
                        pool: self.clone(),
                        inner: InnerBuffer {
                            index: idx,
                            size: size,
                        },
                    });
                }
            }
            Err(NoBufferAvailable)
        } else {
            Err(SizeTooBig)
        }
    }

    fn free_buffer(&self, buffer: &mut InnerBuffer) {
        self.0.buffer_busy[buffer.index].store(false, Ordering::Release);
    }
}

unsafe impl Send for BufferPool {}
unsafe impl Sync for BufferPool {}

#[derive(Debug)]
struct InnerBuffer {
    index: usize,
    size: usize,
}

impl InnerBuffer {
    fn buffer_bounds(&self, pool: &BufferPool) -> std::ops::Range<usize> {
        let buffer_start = pool.0.buffer_size * self.index;
        let buffer_end = buffer_start + self.size;
        buffer_start..buffer_end
    }

    fn buffer_as_slice<'a, 'b, 'c>(&'a self, pool: &'b BufferPool) -> &'c [u8]
        where 'a: 'c, 'b: 'c {
        let range = self.buffer_bounds(pool);
        // Safety:
        // - Self / Buffer is only created by BufferPool::allocate_buffer()
        // - Self / Buffer is not Clone (and not Copy)
        // - ::index and ::size are never modified
        // - ranges returned by ::buffer_bounds() are disjoint as soon as ::index differ
        // TODO: Use slice::chunks()
        unsafe {
            let inner = pool.0.buffers.get().as_ref().unwrap();
            &inner[range]
        }
    }

    fn buffer_as_mut_slice<'a, 'b, 'c>(&'a mut self, pool: &'b BufferPool) -> &'c mut [u8]
        where 'a: 'c, 'b: 'c {
        let range = self.buffer_bounds(pool);
        // Safety:
        // - Self / Buffer is only created by BufferPool::allocate_buffer()
        // - Self / Buffer is not Clone (and not Copy)
        // - ::index and ::size are never modified
        // - ranges returned by ::buffer_bounds() are disjoint as soon as ::index differ
        // TODO: Use slice::chunks_mut()
        unsafe {
            let inner = pool.0.buffers.get().as_mut().unwrap();
            &mut inner[range]
        }
    }
}

// Does not implement Clone. It would be unsafe since cloning would mean allocating the buffer
// space twice.
#[derive(Debug)]
pub struct Buffer {
    pool: BufferPool,
    inner: InnerBuffer,
}

impl fmt::Display for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let bytes = self.deref();
        write!(f, "{:?} / \"", bytes)?;
        for b in bytes {
            write!(f, "{}", char::from(*b))?;
        }
        write!(f, "\"")
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let pool = &self.pool;
        let inner = &mut self.inner;
        pool.free_buffer(inner);
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.inner.buffer_as_slice(&self.pool)
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut [u8] {
        let pool = &self.pool;
        let inner = &mut self.inner;
        inner.buffer_as_mut_slice(pool)
    }
}
