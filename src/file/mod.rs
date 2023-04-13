pub mod protocol;
pub mod receive;
pub mod send;

use std::{fmt, io, net, path};

pub enum DiodeSend {
    Tcp(net::SocketAddr),
    Unix(path::PathBuf),
}

impl fmt::Display for DiodeSend {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Tcp(s) => write!(fmt, "TCP {s}"),
            Self::Unix(p) => write!(fmt, "Unix {}", p.display()),
        }
    }
}

pub struct DiodeReceive {
    pub from_tcp: Option<net::SocketAddr>,
    pub from_unix: Option<path::PathBuf>,
}

impl fmt::Display for DiodeReceive {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(from_tcp) = &self.from_tcp {
            write!(fmt, "TCP {from_tcp}")?;
        }
        if let Some(from_unix) = &self.from_unix {
            write!(fmt, "Unix {}", from_unix.display())?;
        }
        Ok(())
    }
}

pub struct Config<D> {
    pub diode: D,
    pub buffer_size: usize,
    pub hash: bool,
}

pub enum Error {
    Io(io::Error),
    Diode(protocol::Error),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Diode(e) => write!(fmt, "diode error: {e}"),
            Self::Other(e) => write!(fmt, "error: {e}"),
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
