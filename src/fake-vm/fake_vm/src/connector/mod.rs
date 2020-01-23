use binser::{Endianness, FromBytes, FromStream, SizedAsBytes, ToBytes, ToStream, ValidAsBytes, Validate};
use binser_derive::{FromLe, IntoLe, ValidAsBytes, Validate};
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
    fn new(config: &super::Config) -> Result<(Self, Vec<u8>)>;
    fn recv<'a, 'b>(&'a mut self, input_buffer: &'b mut [u8]) -> Result<MsgIn<'b>>;
    fn send<'a, 'b>(&'a mut self, msg: MsgOut<'b>) -> Result<()>;
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

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
enum MsgInType {
    DeliverPacket,
    GoToDeadline,
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

pub enum MsgIn<'a> {
    DeliverPacket(&'a [u8]),
    GoToDeadline(Duration),
}

impl<'a> MsgIn<'a> {
    // No const max macro...
    fn max_header_size() -> usize {
        let max_msg_in_second_buf_size = usize::max(DeliverPacket::NUM_BYTES, GoToDeadline::NUM_BYTES);
        usize::max(MsgInType::NUM_BYTES, max_msg_in_second_buf_size)
    }

    fn recv(src: &mut impl Read, input_buffer: &'a mut [u8], src_endianness: Endianness) -> Result<MsgIn<'a>> {
        let msg_type = MsgInType::from_stream(src, input_buffer, src_endianness)?;
        match msg_type {
            MsgInType::DeliverPacket => {
                let msg = DeliverPacket::from_stream(src, input_buffer, src_endianness)?;
                if let Some(buffer) = input_buffer.get_mut(..(msg.packet.size as usize)) {
                    src.read_exact(buffer)?;
                    Ok(MsgIn::DeliverPacket(buffer))
                } else {
                    Err(Error::new(ErrorKind::InvalidData, "Packet size too big"))
                }
            },
            MsgInType::GoToDeadline => {
                let deadline = GoToDeadline::from_stream(src, input_buffer, src_endianness)?.deadline;
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
        }
    }
}

pub enum MsgOut<'a> {
    AtDeadline,
    SendPacket(Duration, &'a [u8]),
}

impl<'a> MsgOut<'a> {
    // No const max macro...
    fn max_header_size() -> usize {
        usize::max(MsgOutType::NUM_BYTES, SendPacket::NUM_BYTES)
    }

    fn send<'b>(self, dst: &mut impl Write, scratch_buffer: &'b mut [u8], dst_endianness: Endianness) -> Result<()> {
        let msg_type = match self {
            MsgOut::AtDeadline => MsgOutType::AtDeadline,
            MsgOut::SendPacket(_, _) => MsgOutType::SendPacket,
        };
        msg_type.to_stream(dst, scratch_buffer, dst_endianness)?;
        match self {
            MsgOut::AtDeadline => Ok(()),
            MsgOut::SendPacket(send_time, packet) => {
                assert!(packet.len() <= std::u32::MAX as usize);

                let send_packet_header = SendPacket {
                    send_time: Time {
                        seconds: send_time.as_secs(),
                        useconds: send_time.subsec_micros() as u64,
                    },
                    packet: Packet {
                        size: packet.len() as u32,
                    },
                };

                send_packet_header.to_stream(dst, scratch_buffer, dst_endianness)?;
                dst.write_all(packet)
            },
        }
    }
}
