use binser::{Endianness, FromBytes, FromStream, SizedAsBytes, ToBytes, ToStream, ValidAsBytes, Validate};
use binser_derive::{FromLe, IntoLe, ValidAsBytes, Validate};
use crate::buffer_pool::{Buffer, BufferPool};
use std::convert::TryFrom;
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::mem::size_of;
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

// FFI interface, also usable over the network with little-endian encoding

// Incoming messages

#[derive(Clone, Copy, Debug, FromLe, IntoLe, PartialEq, ValidAsBytes, Validate)]
#[repr(C)]
struct Time {
    seconds: u64,
    useconds: u64,
}

#[derive(Clone, Copy, Debug, FromLe, IntoLe, PartialEq, ValidAsBytes, Validate)]
#[repr(C)]
struct Packet {
    size: u32,
}

#[derive(Clone, Copy, Debug, FromLe, IntoLe, PartialEq, ValidAsBytes, Validate)]
#[repr(C)]
struct DeliverPacket {
    packet: Packet,
}

#[derive(Clone, Copy, Debug, FromLe, IntoLe, PartialEq, ValidAsBytes, Validate)]
#[repr(C)]
struct GoToDeadline {
    deadline: Time,
}

// #[repr(C)]
// struct EndSimulation {
// }

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
enum MsgInType {
    DeliverPacket,
    GoToDeadline,
    EndSimulation,
}

impl SizedAsBytes for MsgInType {
    const NUM_BYTES: usize = size_of::<u32>();
}

impl FromBytes for MsgInType {
    fn from_bytes(bytes: &[u8], src_endianness: Endianness) -> Result<(MsgInType, &[u8])> {
        if bytes.len() < MsgInType::NUM_BYTES {
            Err(Error::new(ErrorKind::UnexpectedEof, "Missing data"))
        } else {
            let src_bytes: [u8; MsgInType::NUM_BYTES] = TryFrom::try_from(bytes).unwrap();
            let tmp = match src_endianness {
                Endianness::Native => u32::from_ne_bytes(src_bytes),
                Endianness::Little => u32::from_le_bytes(src_bytes),
            };

            let v = if tmp == MsgInType::DeliverPacket as u32 {
                Some(MsgInType::DeliverPacket)
            } else if tmp == MsgInType::GoToDeadline as u32 {
                Some(MsgInType::GoToDeadline)
            } else if tmp == MsgInType::EndSimulation as u32 {
                Some(MsgInType::EndSimulation)
            } else {
                None
            };
            if let Some(v) = v {
                Ok((v, &bytes[MsgInType::NUM_BYTES..]))
            } else {
                Err(Error::new(ErrorKind::InvalidData, "Invalid message type"))
            }
        }
    }
}

impl ToBytes for MsgInType {
    fn to_bytes(self, bytes: &mut [u8], dst_endianness: Endianness) -> Result<&mut [u8]> {
        if bytes.len() < MsgInType::NUM_BYTES {
            Err(Error::new(ErrorKind::Other, binser::Error::NoSpace))
        } else {
            let dst_bytes = match dst_endianness {
                Endianness::Native => (self as u32).to_ne_bytes(),
                Endianness::Little => (self as u32).to_le_bytes(),
            };
            let (bytes, tail) = bytes.split_at_mut(MsgInType::NUM_BYTES);
            bytes.copy_from_slice(&dst_bytes);
            Ok(tail)
        }
    }
}

// Outgoing messages

// #[repr(C)]
// struct AtDeadline {
// }

#[derive(Clone, Copy, Debug, FromLe, IntoLe, PartialEq, ValidAsBytes, Validate)]
#[repr(C)]
struct SendPacket {
    send_time: Time,
    src: u32,
    dest: u32,
    packet: Packet,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
enum MsgOutType {
    AtDeadline,
    SendPacket,
}

impl SizedAsBytes for MsgOutType {
    const NUM_BYTES: usize = size_of::<u32>();
}

impl FromBytes for MsgOutType {
    fn from_bytes(bytes: &[u8], src_endianness: Endianness) -> Result<(MsgOutType, &[u8])> {
        if bytes.len() < MsgOutType::NUM_BYTES {
            Err(Error::new(ErrorKind::UnexpectedEof, "Missing data"))
        } else {
            let src_bytes: [u8; MsgOutType::NUM_BYTES] = TryFrom::try_from(bytes).unwrap();
            let tmp = match src_endianness {
                Endianness::Native => u32::from_ne_bytes(src_bytes),
                Endianness::Little => u32::from_le_bytes(src_bytes),
            };

            let v = if tmp == MsgOutType::AtDeadline as u32 {
                Some(MsgOutType::AtDeadline)
            } else if tmp == MsgOutType::SendPacket as u32 {
                Some(MsgOutType::SendPacket)
            } else {
                None
            };
            if let Some(v) = v {
                Ok((v, &bytes[MsgOutType::NUM_BYTES..]))
            } else {
                Err(Error::new(ErrorKind::InvalidData, "Invalid message type"))
            }
        }
    }
}

impl ToBytes for MsgOutType {
    fn to_bytes(self, bytes: &mut [u8], dst_endianness: Endianness) -> Result<&mut [u8]> {
        if bytes.len() < MsgOutType::NUM_BYTES {
            Err(Error::new(ErrorKind::Other, binser::Error::NoSpace))
        } else {
            let dst_bytes = match dst_endianness {
                Endianness::Native => (self as u32).to_ne_bytes(),
                Endianness::Little => (self as u32).to_le_bytes(),
            };
            let (bytes, tail) = bytes.split_at_mut(MsgOutType::NUM_BYTES);
            bytes.copy_from_slice(&dst_bytes);
            Ok(tail)
        }
    }
}

// Crate-level interface

fn allocate_buffer(buffer_pool: &BufferPool, size: usize) -> Result<Buffer> {
    buffer_pool.allocate_buffer(size).map_err(|e| match e {
        crate::buffer_pool::Error::SizeTooBig => Error::new(ErrorKind::InvalidData, "Packet size too big"),
        e => Error::new(ErrorKind::Other, e),
    })
}

#[derive(Debug)]
pub enum MsgIn {
    DeliverPacket(Buffer),
    GoToDeadline(Duration),
    EndSimulation,
}

impl MsgIn {
    // No const max macro...
    fn max_header_size() -> usize {
        let max_msg_in_second_buf_size = usize::max(DeliverPacket::NUM_BYTES, GoToDeadline::NUM_BYTES /*, EndSimulation::NUM_BYTES */);
        usize::max(MsgInType::NUM_BYTES, max_msg_in_second_buf_size)
    }

    fn recv<'a, 'b>(src: &mut impl Read, scratch_buffer: &'a mut [u8], buffer_pool: &'b BufferPool, src_endianness: Endianness) -> Result<MsgIn> {
        let msg_type = MsgInType::from_stream(src, scratch_buffer, src_endianness)?;
        match msg_type {
            MsgInType::DeliverPacket => {
                let msg = DeliverPacket::from_stream(src, scratch_buffer, src_endianness)?;
                let mut buffer = allocate_buffer(buffer_pool, msg.packet.size as usize)?;
                src.read_exact(&mut buffer)?;
                Ok(MsgIn::DeliverPacket(buffer))
            },
            MsgInType::GoToDeadline => {
                let deadline = GoToDeadline::from_stream(src, scratch_buffer, src_endianness)?.deadline;
                if let (Ok(seconds), Ok(nseconds)) = (
                    u64::try_from(deadline.seconds),
                    u32::try_from(deadline.useconds).map_err(|_| ()).and_then(|usecs|
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
            MsgInType::EndSimulation => Ok(MsgIn::EndSimulation),
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    fn send<'b>(self, dst: &mut impl Write, scratch_buffer: &'b mut [u8], dst_endianness: Endianness) -> Result<()> {
        let msg_type = match self {
            MsgIn::DeliverPacket(_) => MsgInType::DeliverPacket,
            MsgIn::GoToDeadline(_) => MsgInType::GoToDeadline,
            MsgIn::EndSimulation => MsgInType::EndSimulation,
        };
        msg_type.to_stream(dst, scratch_buffer, dst_endianness)?;
        match self {
            MsgIn::DeliverPacket(packet) => {
                assert!(packet.len() <= std::u32::MAX as usize);

                let deliver_packet_header = DeliverPacket {
                    packet: Packet {
                        size: packet.len() as u32,
                    },
                };

                deliver_packet_header.to_stream(dst, scratch_buffer, dst_endianness)?;
                dst.write_all(&packet)
            },
            MsgIn::GoToDeadline(deadline) => {
                let go_to_deadline = GoToDeadline {
                    deadline: Time {
                        seconds: deadline.as_secs(),
                        useconds: deadline.subsec_micros() as u64,
                    },
                };
                go_to_deadline.to_stream(dst, scratch_buffer, dst_endianness)
            },
            MsgIn::EndSimulation => Ok(()),
        }
    }
}

pub enum MsgOut {
    AtDeadline,
    SendPacket(Duration, u32 , u32, Buffer),
}

impl MsgOut {
    // No const max macro...
    fn max_header_size() -> usize {
        usize::max(MsgOutType::NUM_BYTES, SendPacket::NUM_BYTES)
    }

    fn send(self, dst: &mut impl Write, scratch_buffer: &mut [u8], dst_endianness: Endianness) -> Result<()> {
        let msg_type = match self {
            MsgOut::AtDeadline => MsgOutType::AtDeadline,
            MsgOut::SendPacket(_, _, _, _) => MsgOutType::SendPacket,
        };
        msg_type.to_stream(dst, scratch_buffer, dst_endianness)?;
        match self {
            MsgOut::AtDeadline => Ok(()),
            MsgOut::SendPacket(send_time, src, dest, packet) => {
                assert!(packet.len() <= std::u32::MAX as usize);

                let send_packet_header = SendPacket {
                    send_time: Time {
                        seconds: send_time.as_secs(),
                        useconds: send_time.subsec_micros() as u64,
                    },
                    src: src,
                    dest: dest,
                    packet: Packet {
                        size: packet.len() as u32,
                    },
                };

                send_packet_header.to_stream(dst, scratch_buffer, dst_endianness)?;
                dst.write_all(&packet)
            },
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    fn recv<'a, 'b>(reader: &mut impl Read, scratch_buffer: &'a mut [u8], buffer_pool: &'b BufferPool, src_endianness: Endianness) -> Result<MsgOut> {
        let msg_type = MsgOutType::from_stream(reader, scratch_buffer, src_endianness)?;
        match msg_type {
            MsgOutType::AtDeadline => Ok(MsgOut::AtDeadline),
            MsgOutType::SendPacket => {
                let header = SendPacket::from_stream(reader, scratch_buffer, src_endianness)?;
                if let (Ok(seconds), Ok(nseconds), Ok(src), Ok(dest)) = (
                    u64::try_from(header.send_time.seconds),
                    u32::try_from(header.send_time.useconds).map_err(|_| ()).and_then(|usecs|
                                                                                      if usecs < 1000000 {
                                                                                          Ok(usecs * 1000)
                                                                                      } else {
                                                                                          Err(())
                                                                                      }),
                    u32::try_from(header.src),
                    u32::try_from(header.dest)
                ) {
                    let mut buffer = allocate_buffer(buffer_pool, header.packet.size as usize)?;
                    reader.read_exact(&mut buffer)?;

                    Ok(MsgOut::SendPacket(Duration::new(seconds, nseconds), src, dest, buffer))
                } else {
                    Err(Error::new(ErrorKind::InvalidData, "Time out of bounds"))
                }
            },
        }
    }
}
