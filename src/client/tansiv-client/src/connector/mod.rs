use crate::buffer_pool::{Buffer, BufferPool};
use crate::bytes_buffer::BytesBuffer;
use crate::flatbuilder_buffer::*;
use flatbuffers::{FlatBufferBuilder, Vector, WIPOffset};
use libc::in_addr_t;
use std::convert::TryFrom;
use std::fmt;
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::time::Duration;

pub(super) use unix::*;
#[cfg(any(test, feature = "test-helpers"))]
pub use unix::test_helpers;

mod unix;

pub(crate) type ConnectorImpl = UnixConnector;

pub(crate) trait Connector where Self: Sized {
    fn new(config: &super::Config) -> Result<Self>;
    fn recv(&mut self) -> Result<MsgIn>;
    fn send(&mut self, msg: MsgOut) -> Result<()>;
}

// Crate-level interface
fn allocate_buffer(buffer_pool: &BufferPool<BytesBuffer>, size: usize) -> Result<Buffer<BytesBuffer>> {
    buffer_pool.allocate_buffer(size).map_err(|e| match e {
        crate::buffer_pool::Error::SizeTooBig => Error::new(ErrorKind::InvalidData, "Packet size too big"),
        e => Error::new(ErrorKind::Other, e),
    })
}

mod packets_generated;
pub use packets_generated::*;


#[cfg(any(test, feature = "test-helpers"))]
pub fn create_end_simulation(builder: &mut FlatBufferBuilder) -> () {
    let end_simulation = tansiv::EndSimulation::create(builder, &tansiv::EndSimulationArgs{});
    let msg = tansiv::FromTansivMsg::create(builder, &tansiv::FromTansivMsgArgs{
        content_type: tansiv::FromTansiv::EndSimulation,
        content: Some(end_simulation.as_union_value()),
        ..Default::default()
    });
    builder.finish_size_prefixed(msg, None);
}

#[cfg(any(test, feature = "test-helpers"))]
pub fn create_goto_deadline(builder: &mut FlatBufferBuilder, deadline: Duration) -> () {
    let time = tansiv::Time::new(deadline.as_secs(), deadline.subsec_micros() as u64);
    let goto_deadline = tansiv::GotoDeadline::create(builder, &tansiv::GotoDeadlineArgs {
        time: Some(&time)
    });
    let msg = tansiv::FromTansivMsg::create(builder, &tansiv::FromTansivMsgArgs{
        content_type: tansiv::FromTansiv::GotoDeadline,
        content: Some(goto_deadline.as_union_value()),
        ..Default::default()
    });

    builder.finish_size_prefixed(msg, None);
}

pub fn create_at_deadline(builder: &mut FlatBufferBuilder) -> () {
    let at_deadline = tansiv::AtDeadline::create(builder, &tansiv::AtDeadlineArgs{});
    let msg = tansiv::ToTansivMsg::create(builder, &tansiv::ToTansivMsgArgs{
        content_type: tansiv::ToTansiv::AtDeadline,
        content: Some(at_deadline.as_union_value()),
        ..Default::default()
    });
    builder.finish_size_prefixed(msg, None);
}

#[cfg(any(test, feature = "test-helpers"))]
fn prepare_deliver_packet<'a, 'b, 'c>(builder: &'a mut FlatBufferBuilder<'c>, src: u32, dst: u32, payload: &'b [u8]) -> (&'a mut FlatBufferBuilder<'c>, WIPOffset<tansiv::FromTansivMsg<'c>>) {
    let fb_packet_meta = tansiv::PacketMeta::new(src, dst);
    let fb_payload = builder.create_vector(payload);

    let deliver_packet = tansiv::DeliverPacket::create(
        builder,
        &tansiv::DeliverPacketArgs {
            metadata: Some(&fb_packet_meta),
            payload: Some(fb_payload),
    });
    let msg = tansiv::FromTansivMsg::create(builder, &tansiv::FromTansivMsgArgs{
        content_type: tansiv::FromTansiv::DeliverPacket,
        content: Some(deliver_packet.as_union_value()),
        ..Default::default()
    });

    (builder, msg)
}

#[cfg(any(test, feature = "test-helpers"))]
pub fn create_deliver_packet(builder: &mut FlatBufferBuilder, src: u32, dst: u32, payload: &[u8]) {
    let (builder, msg) = prepare_deliver_packet(builder, src, dst, payload);
    builder.finish_size_prefixed(msg, None);
}

#[cfg(any(test, feature = "test-helpers"))]
pub fn create_deliver_packet_unprefixed(builder: &mut FlatBufferBuilder, src: u32, dst: u32, payload: &[u8]) {
    let (builder, msg) = prepare_deliver_packet(builder, src, dst, payload);
    builder.finish(msg, None);
}

pub fn create_send_packet_from_payload(builder: &mut FlatBufferBuilder, send_time: Duration, src: u32, dst: u32, payload: &[u8]) -> () {
    let time = tansiv::Time::new(send_time.as_secs(), send_time.subsec_micros() as u64);
    let packet_meta = tansiv::PacketMeta::new(src, dst);
    let fb_payload = builder.create_vector(payload);
    let send_packet = tansiv::SendPacket::create(builder, &tansiv::SendPacketArgs {
        metadata: Some(&packet_meta),
        time: Some(&time),
        // FIXME(msimonin) unwrap
        payload: Some(fb_payload),
    });
    let msg = tansiv::ToTansivMsg::create(builder, &tansiv::ToTansivMsgArgs{
        content_type: tansiv::ToTansiv::SendPacket,
        content: Some(send_packet.as_union_value()),
        ..Default::default()
    });
    // write control data
    builder.finish_size_prefixed(msg, None);
}

// Read the actual size of a flatbuffer message
// allocate a scratch buffer on the stack so that
// it's usable from a signal handler
pub fn read_prefixed_size(reader: &mut impl Read) -> Result<usize> {
    let mut scratch = [0u8; 4];
    let _ = reader.read_exact(&mut scratch)?;
    unsafe {
        Ok(flatbuffers::read_scalar::<u32>(&scratch) as usize)
    }
}

#[derive(Debug)]
pub struct MsgFbInitializer;

impl FbBuilderInitializer for MsgFbInitializer {
    fn init<'fbb>(max_packet_size: usize) -> FlatBufferBuilder<'fbb> {
        let mut builder = FlatBufferBuilder::with_capacity(max_packet_size);
        let src = 1u32;
        let dst = 1u32;
        let mut payload = Vec::<u8>::with_capacity(max_packet_size);
        payload.resize(max_packet_size, 1u8);

        let send_time = Duration::new(1u64, 1u32);
        create_send_packet_from_payload(&mut builder, send_time, src, dst, &payload);
        builder.reset();

        create_at_deadline(&mut builder);
        builder.reset();

        builder
   }
}

pub type FbBuffer = FbBuilder<'static, MsgFbInitializer>;


fn new_format_error() -> Error {
    Error::new(ErrorKind::InvalidData, "Message format error")
}

// For all incoming message the wokflow is the same:
// 1. Get the number of bytes to read (encoded as a 4-byte prefix in flatbuffer)
// 2. Allocate a byte buffer of this exact size
// 3. Read this exact number of bytes in this buffer
//
// So when receiving a DeliverPacket we store the raw byte buffer
// corresponding to the serialized message and we provide accessors (src, dst, ...)
// for the actual message field.
//
// Of course, ideally we'd love to wrap a tansiv::DeliverPacket<'a> directly.
// but 'cause of the lifetime  (how surprising), we'd need to propagate some lifetimes...
//
// For other types of message, we can deconstruct the serialized data
#[derive(Debug)]
pub struct DeliverPacket {
    inner: Buffer<BytesBuffer>
}

impl DeliverPacket {
    pub fn src(&self) -> u32 {
        let msg = self.deserialize();
        msg.metadata().unwrap().src()
    }

    pub fn dst(&self) -> u32 {
        let msg = self.deserialize();
        msg.metadata().unwrap().dst()
    }

    pub fn payload(&self) -> &[u8] {
        let msg = self.deserialize();
        msg.payload().unwrap().bytes()
    }

    fn deserialize(&self) -> tansiv::DeliverPacket {
        // - can we assume that it has been verified prior to this ?
        // (connector.recv is doing the check)
        // if yes we can cool root_unchecked
        let msg = flatbuffers::root::<tansiv::FromTansivMsg>(&self.inner).unwrap();
        msg.content_as_deliver_packet().unwrap()
    }
}

#[derive(Debug)]
pub enum MsgIn {
    DeliverPacket(DeliverPacket),
    GoToDeadline(Duration),
    EndSimulation,
}

impl MsgIn {
    pub fn new_deliver_packet(buffer: Buffer<BytesBuffer>) -> Result<MsgIn> {
        // checking that we're dealing with the right message
        let msg = flatbuffers::root::<tansiv::FromTansivMsg>(&buffer).unwrap();
        let msg = msg.content_as_deliver_packet().ok_or(new_format_error())?;
        // we don't trust fbb, so we check all the field
        if msg.metadata().and(msg.payload()).is_none() {
            return Err(new_format_error());
        }

        Ok(
            MsgIn::DeliverPacket(DeliverPacket {
                inner: buffer
            })
        )
    }
}

impl fmt::Display for MsgIn {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use crate::vsg_address::to_ipv4addr;
        match self {
            // FIXME(msimonin) // real payload
            MsgIn::DeliverPacket(d) => {
                write!(f, "DeliverPacket(src = {}, dst = {}, len = {}", to_ipv4addr(d.src()), to_ipv4addr(d.dst()), d.payload().len())
            },
            _ => fmt::Debug::fmt(self, f),
        }
    }
}

impl MsgIn {
    fn recv<'a, 'b>(reader: &mut impl Read, buffer_pool: &'b BufferPool<BytesBuffer>) -> Result<MsgIn> {
        let size = read_prefixed_size(reader)?;

        let mut  buffer = allocate_buffer(buffer_pool, size)?;
        reader.read_exact(&mut buffer)?;

        let msg = flatbuffers::root::<tansiv::FromTansivMsg>(&buffer)
            .map_err(|_| {new_format_error()})?;
        match msg.content_type() {
            tansiv::FromTansiv::DeliverPacket => MsgIn::new_deliver_packet(buffer),
            tansiv::FromTansiv::GotoDeadline => {
                let deadline = msg.content_as_goto_deadline().ok_or(new_format_error())?;
                let time = deadline.time().ok_or(new_format_error())?;
                if let (Ok(seconds), Ok(nseconds)) = (
                    u64::try_from(time.seconds()),
                    u32::try_from(time.useconds()).map_err(|_| ()).and_then(|usecs|
                                                        if usecs < 1000000 {
                                                            Ok(usecs * 1000)
                                                        } else {
                                                            Err(())
                                                        })
                ) {
                    Ok(MsgIn::GoToDeadline(Duration::new(seconds, nseconds)))
                } else {
                    Err(Error::new(ErrorKind::InvalidData, "Time out of bounds"))
                }
            },
            tansiv::FromTansiv::EndSimulation => Ok(MsgIn::EndSimulation),
            _ => Err(Error::new(ErrorKind::InvalidData, "Message content is missing")),
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    fn send<'b>(self, writer: &mut impl Write, fb_buffer_pool: &BufferPool<FbBuffer>) -> Result<()> {
        let mut fb_buffer = fb_buffer_pool.allocate_buffer(1).unwrap();
        // at this point d is unprefixed !
        // intial implem:
        // - we deserialize the buffer to reserialize it with a size prefixed
        match self {
            MsgIn::GoToDeadline(deadline) => {
                create_goto_deadline(&mut fb_buffer, deadline);
                writer.write_all(fb_buffer.finished_data())
            },
            MsgIn::EndSimulation => {
                create_end_simulation(&mut fb_buffer);
                writer.write_all(fb_buffer.finished_data())
            }
            MsgIn::DeliverPacket(d) => {
                // NOTE(msimonin): the byte buffer here correspond to an unprefixed deliverPacket on the wire
                // the intend here is to craft a prefixed message and send it over the wire
                create_deliver_packet(&mut fb_buffer, d.src(), d.dst(), d.payload());
                writer.write_all(fb_buffer.finished_data())
            }
        }
    }
}

// Use for representing a partially built buffer
pub struct SendPacketBuilder {
    src: in_addr_t,
    dst: in_addr_t,
    send_time: Duration,
    payload: Buffer<FbBuffer>,
    payload_offset: WIPOffset<Vector<'static, u8>>,
}

impl SendPacketBuilder {
    pub fn new(src: in_addr_t, dst: in_addr_t, send_time: Duration, payload: &[u8], mut buffer: Buffer<FbBuffer>) -> Result<SendPacketBuilder> {

        let payload_offset = buffer.create_vector(payload);
        Ok(SendPacketBuilder {
            src,
            dst,
            send_time,
            payload: buffer,
            payload_offset,
        })
    }

    pub fn finish(self, send_time: Duration) -> SendPacket {
        let time = tansiv::Time::new(send_time.as_secs(), send_time.subsec_micros() as u64);
        let packet_meta = tansiv::PacketMeta::new(self.src, self.dst);
        let mut p = self.payload;
        let send_packet = tansiv::SendPacket::create(&mut p, &tansiv::SendPacketArgs {
            metadata: Some(&packet_meta),
            time: Some(&time),
            payload: Some(self.payload_offset),
        });
        let msg = tansiv::ToTansivMsg::create(&mut p, &tansiv::ToTansivMsgArgs{
            content_type: tansiv::ToTansiv::SendPacket,
            content: Some(send_packet.as_union_value()),
            ..Default::default()
        });
        p.finish_size_prefixed(msg, None);
        SendPacket {
            inner: p
        }
    }

    pub fn src(&self) -> in_addr_t {
        self.src
    }
    pub fn dst(&self) -> in_addr_t{
        self.dst
    }

    pub fn send_time(&self) -> Duration {
        self.send_time
    }
}

// Used to represent a fully built buffer
#[derive(Debug)]
pub struct SendPacket {
    inner: Buffer<FbBuffer>,
}

impl std::ops::Deref for SendPacket {
    type Target = Buffer<FbBuffer>;
    fn deref(&self) -> &Buffer<FbBuffer> {
        &self.inner
    }
}


#[derive(Debug)]
pub enum MsgOut {
    AtDeadline,
    SendPacket(SendPacket),
}

impl MsgOut {
    fn send(mut self, writer: &mut impl Write, scratch_builder: &mut FlatBufferBuilder<'static>) -> Result<()> {
        let fbb = match self {
            MsgOut::AtDeadline => {
                create_at_deadline(scratch_builder);
                scratch_builder
            },
            MsgOut::SendPacket(SendPacket { ref mut inner }) => inner,
        };
        writer.write_all(fbb.finished_data())
    }

    #[cfg(any(test, feature = "test-helpers"))]
    fn recv<'a, 'b>(reader: &mut impl Read, buffer_pool: &'b BufferPool<BytesBuffer>, fb_buffer_pool: &BufferPool<FbBuffer>) -> Result<MsgOut> {
        let size = read_prefixed_size(reader)?;

        let mut buffer = allocate_buffer(buffer_pool, size)?;
        reader.read_exact(&mut buffer)?;
        let msg = flatbuffers::root::<tansiv::ToTansivMsg>(&buffer).unwrap();

        let fb_buffer = fb_buffer_pool.allocate_buffer(0).unwrap();
        match msg.content_type() {
            tansiv::ToTansiv::AtDeadline =>  Ok(MsgOut::AtDeadline),
            tansiv::ToTansiv::SendPacket => {
                let send_packet = msg.content_as_send_packet().ok_or(new_format_error())?;
                let time = send_packet.time().ok_or(new_format_error())?;
                let metadata = send_packet.metadata().ok_or(new_format_error())?;
                if let (Ok(seconds), Ok(nseconds)) = (
                    u64::try_from(time.seconds()),
                    u32::try_from(time.useconds()).map_err(|_| ()).and_then(|usecs|
                                                        if usecs < 1000000 {
                                                            Ok(usecs * 1000)
                                                        } else {
                                                            Err(())
                                                        })
                ){
                    let send_time = Duration::new(seconds, nseconds);
                    let send_packet_builder = SendPacketBuilder::new(
                        metadata.src(),
                        metadata.dst(),
                        send_time,
                        send_packet.payload().unwrap().bytes(),
                        fb_buffer,
                    )?;
                    Ok(MsgOut::SendPacket(send_packet_builder.finish(send_time)))
                } else {
                    Err(Error::new(ErrorKind::InvalidData, "Time out of bounds"))
                }
            },
            _ => Err(Error::new(ErrorKind::InvalidData, "Message content is missing")),
        }
    }
}


#[cfg(test)]
mod test {
    #[allow(unused_imports)]
    use flatbuffers::FlatBufferBuilder;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::time::Duration;

    use super::*;

    static mut N_ALLOCS: usize = 0;

    struct TrackingAllocator;

    impl TrackingAllocator {
        fn n_allocs(&self) -> usize {
            unsafe { N_ALLOCS }
        }
    }
    unsafe impl GlobalAlloc for TrackingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            N_ALLOCS += 1;
            System.alloc(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout)
        }
    }

    // use the tracking allocator:
    #[global_allocator]
    static A: TrackingAllocator = TrackingAllocator;

    macro_rules! assert_no_alloc {
    ($alloc : expr, $myblock : block) => {
        {
            let before = $alloc.n_allocs();
            $myblock
            let after = $alloc.n_allocs();
            assert_eq!(before, after);
        }
    }
}


    #[test]
    fn alloc_fb_send_test() {
        let fb_pool = crate::BufferPool::<FbBuilder<MsgFbInitializer>>::new(crate::MAX_PACKET_SIZE, 10);

        let mtu = 1500;
        let payload = &vec![1u8; mtu];
        let d = Duration::new(1u64, 1u32);

        {
            // at deadline
            assert_no_alloc!(A, {
                let mut fb: Buffer<FbBuffer> = fb_pool.allocate_buffer(0).unwrap();
                create_at_deadline(&mut fb);
            });
        }

        {
            // goto deadline
            assert_no_alloc!(A, {
                let mut fb: Buffer<FbBuffer> = fb_pool.allocate_buffer(0).unwrap();
                let d = Duration::new(0u64, 0u32);
                create_goto_deadline(&mut fb, d);
            });
        }

        {
            // send packet
            assert_no_alloc!(A, {
                let fb: Buffer<FbBuffer> = fb_pool.allocate_buffer(0).unwrap();
                let send_packet_builder = SendPacketBuilder::new(1u32, 1u32, d, payload, fb).unwrap();
                send_packet_builder.finish(d);
            });
        }

        {
            // send packet
            assert_no_alloc!(A, {
                let fb: Buffer<FbBuffer> = fb_pool.allocate_buffer(0).unwrap();
                let send_packet_builder = SendPacketBuilder::new(0u32, 1u32, d, payload, fb).unwrap();
                let _ = MsgOut::SendPacket(send_packet_builder.finish(d));
            });
        }



        {
            assert_no_alloc!(A, {
                let mut fb: Buffer<FbBuffer> = fb_pool.allocate_buffer(0).unwrap();
                create_deliver_packet(&mut fb, 1u32, 1u32, payload);
            });
        }

    }

    #[test]
    fn alloc_fb_receive() {

        let mtu = 1500;
        let mut builder = FlatBufferBuilder::new();

        {
            create_deliver_packet(&mut builder, 0u32, 1u32, &vec![1u8; mtu]);
            let buffer = builder.finished_data();

            assert_no_alloc!(A, {
                let _ = flatbuffers::root::<tansiv::FromTansivMsg>(&buffer);
                builder.reset();
            });
        }

        {
            let d = Duration::new(0u64, 1u32);
            create_goto_deadline(&mut builder, d);
            let buffer = builder.finished_data();

            assert_no_alloc!(A, {
                let _ = flatbuffers::root::<tansiv::FromTansivMsg>(&buffer);
                builder.reset();
            });
        }
    }
}
