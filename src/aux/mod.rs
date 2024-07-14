pub mod file;
pub mod udp;

use std::{fmt, net, path};

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
