//! Sender functions module
//!
//! Several threads are used to form a pipeline for the data to be prepared before sending it over
//! UDP. Every submodule of the [`crate::send`] module is equipped with a `start` function that
//! launch the worker process. Data pass through the workers pipelines via [`crossbeam_channel`]
//! bounded channels.
//!
//! Here follows a simplified representation of the workers pipeline:
//!
//! ```text
//!             ----------             ---------
//! listeners --| client |-> clients --| block |-> udp
//!             ----------             ---------
//! ```
//!
//! Notes:
//! - listeners threads are spawned from binary and not the library crate,
//! - heartbeat worker has been omitted from the representation for readability,
//! - there are `max_clients` clients workers running in parallel,

use crate::protocol;
use std::{
    fmt,
    io::{self, Read},
    net,
    os::fd::AsRawFd,
    sync, thread, time,
};

mod client;
mod heartbeat;
mod server;
mod udp;

pub struct Config {
    pub max_clients: protocol::ClientId,
    pub flush: bool,
    pub heartbeat_interval: Option<time::Duration>,
    pub to: net::IpAddr,
    pub to_ports: Vec<u16>,
    pub to_bind: net::SocketAddr,
    pub to_mtu: u16,
    pub mode: crate::SendMode,
    #[cfg(feature = "transfer-hash")]
    pub hash: bool,
}

pub enum Error {
    Io(io::Error),
    SendBlock,
    SendUdp,
    Receive(crossbeam_channel::RecvError),
    Protocol(protocol::Error),
    Diode(String),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::SendBlock => write!(fmt, "crossbeam send block error"),
            Self::SendUdp => write!(fmt, "crossbeam send UDP error"),
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::Protocol(e) => write!(fmt, "diode protocol error: {e}"),
            Self::Diode(e) => write!(fmt, "diode error: {e}"),
            Self::Other(e) => write!(fmt, "{e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crossbeam_channel::SendError<Option<protocol::Block>>> for Error {
    fn from(_: crossbeam_channel::SendError<Option<protocol::Block>>) -> Self {
        Self::SendBlock
    }
}

impl From<crossbeam_channel::SendError<Option<(u8, protocol::Block)>>> for Error {
    fn from(_: crossbeam_channel::SendError<Option<(u8, protocol::Block)>>) -> Self {
        Self::SendBlock
    }
}

impl From<crossbeam_channel::SendError<Option<(u8, Vec<raptorq::EncodingPacket>)>>> for Error {
    fn from(_: crossbeam_channel::SendError<Option<(u8, Vec<raptorq::EncodingPacket>)>>) -> Self {
        Self::SendUdp
    }
}

impl From<crossbeam_channel::SendError<Option<Vec<raptorq::EncodingPacket>>>> for Error {
    fn from(_: crossbeam_channel::SendError<Option<Vec<raptorq::EncodingPacket>>>) -> Self {
        Self::SendUdp
    }
}

impl From<crossbeam_channel::RecvError> for Error {
    fn from(e: crossbeam_channel::RecvError) -> Self {
        Self::Receive(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Protocol(e)
    }
}

/// An instance of this data structure is shared by workers to synchronize them and to access
/// communication channels
///
/// The `C` type variable represents the socket from which data is read before being sent over the
/// diode.
pub struct Sender<C> {
    config: Config,
    raptorq: protocol::RaptorQ,
    multiplex_control: semka::Sem,
    next_block: sync::atomic::AtomicU8,
    to_server: crossbeam_channel::Sender<Option<C>>,
    for_server: crossbeam_channel::Receiver<Option<C>>,
    to_udp: crossbeam_channel::Sender<Option<(u8, protocol::Block)>>,
    for_udp: crossbeam_channel::Receiver<Option<(u8, protocol::Block)>>,
}

impl<C> Sender<C>
where
    C: Read + AsRawFd + Send,
{
    /// # Errors
    ///
    /// Will return `Err` if `multiplex_control` semaphore
    /// cannot be created.
    pub fn new(config: Config, raptorq: protocol::RaptorQ) -> Result<Self, Error> {
        let multiplex_control = semka::Sem::new(config.max_clients)
            .ok_or(Error::Other("failed to create semaphore".into()))?;

        if config.to_mtu > crate::MAX_MTU {
            return Err(Error::Other(format!(
                "mtu {} is too large (> {})",
                config.to_mtu,
                crate::MAX_MTU
            )));
        }

        let next_block = sync::atomic::AtomicU8::new(0);

        let (to_server, for_server) = crossbeam_channel::bounded(1);
        let (to_udp, for_udp) = crossbeam_channel::bounded(config.to_ports.len());

        Ok(Self {
            config,
            raptorq,
            multiplex_control,
            next_block,
            to_server,
            for_server,
            to_udp,
            for_udp,
        })
    }

    /// # Errors
    ///
    /// Will return `Err` if scoped threads cannot spawned.
    pub fn start<'a>(&'a self, scope: &'a thread::Scope<'a, '_>) -> Result<(), Error> {
        log::info!(
            "max {} simultaneous clients/transfers",
            self.config.max_clients
        );

        log::info!("send mode is {}", self.config.mode);

        for port in &self.config.to_ports {
            thread::Builder::new()
                .name(format!("udp_{port}"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = udp::start(self, *port) {
                        log::error!("fatal udp error: {e}");
                    }
                })?;
        }

        if let Some(hb_interval) = self.config.heartbeat_interval {
            log::info!(
                "heartbeat block will be sent every {} seconds",
                hb_interval.as_secs()
            );
            thread::Builder::new()
                .name("heartbeat".into())
                .spawn_scoped(scope, move || {
                    if let Err(e) = heartbeat::start(self) {
                        log::error!("fatal heartbeat error; {e}");
                    }
                })?;
        } else {
            log::info!("heartbeat is disabled");
        }

        for i in 0..self.config.max_clients {
            thread::Builder::new()
                .name(format!("client_{i}"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = server::start(self) {
                        log::error!("fatal client_{i} error: {e}");
                    }
                })?;
        }

        log::info!(
            "RaptorQ block {} bytes splitted in {}/{} packets",
            self.raptorq.block_size(),
            self.raptorq.min_nb_packets(),
            self.raptorq.nb_packets()
        );

        log::debug!("{}", self.raptorq);

        Ok(())
    }
    /// # Errors
    ///
    /// Will return `Err` if the `send` returns a `SendError<T>`.
    pub fn new_client(&self, client: C) -> Result<(), Error> {
        if let Err(e) = self.to_server.send(Some(client)) {
            return Err(Error::Diode(format!("failed to enqueue client: {e}")));
        }
        Ok(())
    }
    /// # Errors
    ///
    /// Will return `Err` if the `send` returns a `SendError<T>`.
    pub fn stop(&self) -> Result<(), Error> {
        if let Err(e) = self.to_server.send(None) {
            return Err(Error::Diode(format!("failed to stop: {e}")));
        }
        Ok(())
    }
}
