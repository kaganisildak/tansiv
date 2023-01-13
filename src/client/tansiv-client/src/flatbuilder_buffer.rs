use crate::buffer_pool::{InnerBuffer, InnerBufferPool};
use flatbuffers::{FlatBufferBuilder};
use std::marker::PhantomData;
use std::cell::UnsafeCell;

pub trait FbBuilderInitializer {
    fn init<'fbb>(max_packet_size: usize) -> FlatBufferBuilder<'fbb>;
}


#[derive(Debug)]
pub struct FbBuilder<'fbb, I> {
    _phantom: PhantomData<(&'fbb (), I)>
}

impl<'fbb, I> InnerBuffer for FbBuilder<'fbb, I> where I: FbBuilderInitializer {

    type Array = UnsafeCell<Vec<FlatBufferBuilder<'fbb>>>;
    type Content = FlatBufferBuilder<'fbb>;

    fn calloc(buffer_size: usize, num_buffers: usize) -> Self::Array {
        let mut buffers: Vec<FlatBufferBuilder> =  Vec::with_capacity(num_buffers);
        buffers.resize_with(num_buffers, || -> FlatBufferBuilder { I::init(buffer_size) });
        UnsafeCell::new(buffers)
    }

    fn new(_index: usize, _size: usize) -> Self {
        FbBuilder {
            _phantom: PhantomData
        }
    }

    fn get<'a, 'b, 'c>(&'a self, pool: &'b InnerBufferPool<Self>, index: usize) -> &'c Self::Content
        where 'a: 'c, 'b: 'c {
        unsafe {
            &pool.buffers().get().as_ref().unwrap()[index]
        }
    }


    fn get_mut<'a, 'b, 'c>(&'a mut self, pool: &'b InnerBufferPool<Self>, index: usize) -> &'c mut Self::Content
        where 'a: 'c, 'b: 'c {
        unsafe {
            let inner = pool.buffers().get().as_mut().unwrap();
            &mut inner[index]
        }
    }

    fn reset<'a, 'b>(&'a mut self, pool: &'b InnerBufferPool<Self>, index: usize) where 'a: 'b {
        self.get_mut(pool, index).reset();
    }
}

