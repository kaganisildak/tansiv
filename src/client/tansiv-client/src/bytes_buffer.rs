use crate::buffer_pool::{InnerBuffer, InnerBufferDisplay, InnerBufferPool};
use std::cell::UnsafeCell;
use std::fmt;

#[derive(Debug)]
pub struct BytesBuffer {
    size: usize,
}

impl BytesBuffer {
    fn buffer_bounds(&self, pool: &InnerBufferPool<Self>, index: usize) -> std::ops::Range<usize> {
        let buffer_start = pool.buffer_size() * index;
        let buffer_end = buffer_start + self.size;
        buffer_start..buffer_end
    }
}

impl InnerBuffer for BytesBuffer {
    type Array = UnsafeCell<Vec<u8>>;

    fn calloc(buffer_size: usize, num_buffers: usize) -> Self::Array {
        let mut buffers: Vec<u8> = Vec::with_capacity(buffer_size * num_buffers);
        buffers.resize(buffer_size * num_buffers, 0);
        UnsafeCell::new(buffers)
    }

    fn new(_index: usize, size: usize) -> Self {
        BytesBuffer {
            size,
        }
    }

    fn as_slice<'a, 'b, 'c>(&'a self, pool: &'b InnerBufferPool<Self>, index: usize) -> &'c [u8]
        where 'a: 'c, 'b: 'c {
        let range = self.buffer_bounds(pool, index);
        // Safety:
        // - Only called by Buffer::deref()
        // - Self / Buffer is only created by BufferPool::allocate_buffer()
        // - Self / Buffer is not Clone (and not Copy)
        // - ::index and ::size are never modified
        // - ranges returned by ::buffer_bounds() are disjoint as soon as ::index differ
        // TODO: Use slice::chunks()
        unsafe {
            let inner = pool.buffers().get().as_ref().unwrap();
            &inner[range]
        }
    }

    fn as_mut_slice<'a, 'b, 'c>(&'a mut self, pool: &'b InnerBufferPool<Self>, index: usize) -> &'c mut [u8]
        where 'a: 'c, 'b: 'c {
        let range = self.buffer_bounds(pool, index);
        // Safety:
        // - Only called by Buffer::deref_mut()
        // - Self / Buffer is only created by BufferPool::allocate_buffer()
        // - Self / Buffer is not Clone (and not Copy)
        // - ::index and ::size are never modified
        // - ranges returned by ::buffer_bounds() are disjoint as soon as ::index differ
        // TODO: Use slice::chunks_mut()
        unsafe {
            let inner = pool.buffers().get().as_mut().unwrap();
            &mut inner[range]
        }
    }
}

impl InnerBufferDisplay for BytesBuffer {
    fn display(&self, content: &[u8], f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} / \"", content)?;
        for b in content {
            write!(f, "{}", char::from(*b))?;
        }
        write!(f, "\"")
    }
}
