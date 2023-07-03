use std::{error, fmt, io};

#[derive(Debug)]
pub enum Error {
    AlreadyStarted,
    FlowControlLimited,
    NoMemoryAvailable,
    NoMessageAvailable,
    ProtocolViolation,
    SizeTooBig,
    IoError(io::Error),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::IoError(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::IoError(e) => e.fmt(f),
            simple => {
                let msg = match simple {
                    Error::AlreadyStarted => "Already Started",
                    Error::FlowControlLimited => "Flow Control Limited",
                    Error::NoMemoryAvailable => "No memory available",
                    Error::NoMessageAvailable => "No message available",
                    Error::ProtocolViolation => "Protocol violation",
                    Error::SizeTooBig => "Size too big",
                    Error::IoError(_) => unimplemented!(),
                };
                write!(f, "{}", msg)
            },
        }
    }
}

impl error::Error for Error {}
