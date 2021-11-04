use std::fmt;
use std::io::{Error as IoError, ErrorKind, Read, Result, Write};
use std::mem::size_of;

#[derive(Clone, Copy, Debug)]
pub enum Error {
    NoSpace,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::NoSpace => write!(f, "Not enough space"),
        }
    }
}

impl std::error::Error for Error {
}

#[derive(Clone, Copy, Debug)]
pub enum Endianness {
    Native,
    Little,
}

pub trait FromLe {
    fn from_le(x: Self) -> Self;
}

impl FromLe for i8 {
    fn from_le(x: i8) -> i8 {
        i8::from_le(x)
    }
}

impl FromLe for i16 {
    fn from_le(x: i16) -> i16 {
        i16::from_le(x)
    }
}

impl FromLe for i32 {
    fn from_le(x: i32) -> i32 {
        i32::from_le(x)
    }
}

impl FromLe for i64 {
    fn from_le(x: i64) -> i64 {
        i64::from_le(x)
    }
}

impl FromLe for i128 {
    fn from_le(x: i128) -> i128 {
        i128::from_le(x)
    }
}

impl FromLe for isize {
    fn from_le(x: isize) -> isize {
        isize::from_le(x)
    }
}

impl FromLe for u8 {
    fn from_le(x: u8) -> u8 {
        u8::from_le(x)
    }
}

impl FromLe for u16 {
    fn from_le(x: u16) -> u16 {
        u16::from_le(x)
    }
}

impl FromLe for u32 {
    fn from_le(x: u32) -> u32 {
        u32::from_le(x)
    }
}

impl FromLe for u64 {
    fn from_le(x: u64) -> u64 {
        u64::from_le(x)
    }
}

impl FromLe for u128 {
    fn from_le(x: u128) -> u128 {
        u128::from_le(x)
    }
}

impl FromLe for usize {
    fn from_le(x: usize) -> usize {
        usize::from_le(x)
    }
}

pub trait IntoLe {
    fn into_le(self) -> Self;
}

impl IntoLe for i8 {
    fn into_le(self) -> i8 {
        self.to_le()
    }
}

impl IntoLe for i16 {
    fn into_le(self) -> i16 {
        self.to_le()
    }
}

impl IntoLe for i32 {
    fn into_le(self) -> i32 {
        self.to_le()
    }
}

impl IntoLe for i64 {
    fn into_le(self) -> i64 {
        self.to_le()
    }
}

impl IntoLe for i128 {
    fn into_le(self) -> i128 {
        self.to_le()
    }
}

impl IntoLe for isize {
    fn into_le(self) -> isize {
        self.to_le()
    }
}

impl IntoLe for u8 {
    fn into_le(self) -> u8 {
        self.to_le()
    }
}

impl IntoLe for u16 {
    fn into_le(self) -> u16 {
        self.to_le()
    }
}

impl IntoLe for u32 {
    fn into_le(self) -> u32 {
        self.to_le()
    }
}

impl IntoLe for u64 {
    fn into_le(self) -> u64 {
        self.to_le()
    }
}

impl IntoLe for u128 {
    fn into_le(self) -> u128 {
        self.to_le()
    }
}

impl IntoLe for usize {
    fn into_le(self) -> usize {
        self.to_le()
    }
}

pub trait SizedAsBytes {
    const NUM_BYTES: usize;
}

pub trait FromBytes: Sized {
    fn from_bytes<'a>(bytes: &'a [u8], src_endianness: Endianness) -> Result<(Self, &'a [u8])>;
}

pub trait FromStream: Sized {
    fn from_stream(stream: &mut impl Read, scratch_buffer: &mut [u8], src_endianness: Endianness) -> Result<Self>;
}

pub trait Validate {
    fn validate(_v: *const Self, _endianness: Endianness) -> Result<()> {
        Ok(())
    }
}

impl<T> SizedAsBytes for T where T: Sized + ValidAsBytes {
    const NUM_BYTES: usize = size_of::<Self>();
}

impl<T> FromBytes for T where T: Validate + FromLe {
    fn from_bytes(bytes: &[u8], src_endianness: Endianness) -> Result<(T, &[u8])> {
        if bytes.len() < size_of::<T>() {
            Err(IoError::new(ErrorKind::UnexpectedEof, "Missing data"))
        } else {
            let ptr = bytes.as_ptr() as *const T;
            T::validate(ptr, src_endianness)?;
            let v = unsafe { std::ptr::read_unaligned(ptr) };
            let v = match src_endianness {
                Endianness::Native => v,
                Endianness::Little => T::from_le(v),
            };
            Ok((v, &bytes[size_of::<T>()..]))
        }
    }
}

impl<T> FromStream for T where T: SizedAsBytes + FromBytes {
    fn from_stream(stream: &mut impl Read, scratch_buffer: &mut [u8], src_endianness: Endianness) -> Result<T> {
        if let Some(buffer) = scratch_buffer.get_mut(..T::NUM_BYTES) {
            stream.read_exact(buffer)?;
            let (v, _) = T::from_bytes(buffer, src_endianness)?;
            Ok(v)
        } else {
            Err(IoError::new(ErrorKind::Other, Error::NoSpace))
        }
    }
}

pub trait ToBytes {
    fn to_bytes<'a>(self, bytes: &'a mut [u8], dst_endianness: Endianness) -> Result<&'a mut [u8]>;
}

pub trait ToStream {
    fn to_stream<'a, 'b>(self, dst: &'a mut impl Write, scratch_buffer: &'b mut [u8], dst_endianness: Endianness) -> Result<()>;
}

pub trait ValidAsBytes {
}

impl ValidAsBytes for i8 {
}

impl ValidAsBytes for i16 {
}

impl ValidAsBytes for i32 {
}

impl ValidAsBytes for i64 {
}

impl ValidAsBytes for i128 {
}

impl ValidAsBytes for isize {
}

impl ValidAsBytes for u8 {
}

impl ValidAsBytes for u16 {
}

impl ValidAsBytes for u32 {
}

impl ValidAsBytes for u64 {
}

impl ValidAsBytes for u128 {
}

impl ValidAsBytes for usize {
}

impl<T> ToBytes for T where T: Sized + ValidAsBytes + IntoLe {
    fn to_bytes(self, bytes: &mut [u8], dst_endianness: Endianness) -> Result<&mut [u8]> {
        if bytes.len() < size_of::<T>() {
            Err(IoError::new(ErrorKind::Other, Error::NoSpace))
        } else {
            let v = match dst_endianness {
                Endianness::Native => self,
                Endianness::Little => self.into_le(),
            };
            unsafe { std::ptr::write_unaligned(bytes.as_mut_ptr() as *mut T, v) };
            Ok(&mut bytes[size_of::<T>()..])
        }
    }
}

impl<T> ToStream for T where T: ToBytes {
    fn to_stream<'a, 'b>(self, stream: &'a mut impl Write, scratch_buffer: &'b mut [u8], dst_endianness: Endianness) -> Result<()> {
        let next_buffer = self.to_bytes(scratch_buffer, dst_endianness)?;
        let remaining_len = next_buffer.len();
        stream.write_all(&scratch_buffer[..(scratch_buffer.len() - remaining_len)])
    }
}
