//! Module for sending/receiving entire files into/from Lidi TCP or Unix sockets

#[cfg(feature = "tls")]
use crate::tls;
use std::{fmt, io, num};

pub mod protocol;
pub mod receive;
pub mod send;

pub struct Config<D> {
    pub diode: D,
    pub buffer_size: usize,
    #[cfg(feature = "hash")]
    pub hash: bool,
    pub max_files: usize,
    pub overwrite: bool,
    pub ignore: Option<regex::Regex>,
    #[cfg(feature = "inotify")]
    pub watch: bool,
    pub tls: crate::Tls,
}

pub enum Error {
    Io(io::Error),
    Diode(protocol::Error),
    #[cfg(feature = "tls")]
    Tls(tls::Error),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Diode(e) => write!(fmt, "diode error: {e}"),
            #[cfg(feature = "tls")]
            Self::Tls(e) => write!(fmt, "TLS error: {e}"),
            Self::Other(e) => write!(fmt, "{e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Diode(e)
    }
}

impl From<num::TryFromIntError> for Error {
    fn from(e: num::TryFromIntError) -> Self {
        Self::Other(e.to_string())
    }
}

#[cfg(feature = "tls")]
impl From<tls::Error> for Error {
    fn from(e: tls::Error) -> Self {
        Self::Tls(e)
    }
}
