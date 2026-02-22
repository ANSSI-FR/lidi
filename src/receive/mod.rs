//! Receiver functions module
//!
//! Several threads are involved in the receipt pipeline. Each worker is run with a `start`
//! function of a submodule of the [`crate::receive`] module, data being passed through
//! [`crossbeam_channel`] bounded channels to form the following data pipeline:
//!
//! ```text
//!       -----------             ------------------            ---------
//! udp --| packets |-> reblock --| vec of packets |-> decode --| block |-> dispatch
//!       -----------             ------------------            ---------
//! ```
//!
//! Notes:
//! - heartbeat does not need a dedicated worker on the receiver side, heartbeat blocks are
//!   handled by the dispatch worker,
//! - there are `max_clients` clients workers running in parallel,
//! - there are `nb_decode_threads` decode workers running in parallel.

use crate::protocol;
use std::{
    fmt,
    io::{self, Write},
    net,
    os::fd::AsRawFd,
    thread, time,
};

mod client;
mod clients;
mod decode;
mod dispatch;
mod reblock;
mod udp;

pub struct Config {
    pub from: net::IpAddr,
    pub from_ports: Vec<u16>,
    pub from_mtu: u16,
    pub mode: crate::RecvMode,
    pub reset_timeout: time::Duration,
    pub max_clients: protocol::ClientId,
    pub flush: bool,
    pub abort_timeout: Option<time::Duration>,
    pub heartbeat_interval: Option<time::Duration>,
    #[cfg(feature = "transfer-hash")]
    pub hash: bool,
}

pub enum Error {
    Io(io::Error),
    SendPackets,
    SendBlockPackets,
    SendBlock,
    SendClients,
    Receive(crossbeam_channel::RecvError),
    ReceiveTimeout(crossbeam_channel::RecvTimeoutError),
    Protocol(protocol::Error),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::SendPackets => write!(fmt, "crossbeam send packets error"),
            Self::SendBlockPackets => write!(fmt, "crossbeam send block packets error"),
            Self::SendBlock => write!(fmt, "crossbeam send block error"),
            Self::SendClients => write!(fmt, "crossbeam send client error"),
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::ReceiveTimeout(e) => write!(fmt, "crossbeam receive timeout error: {e}"),
            Self::Protocol(e) => write!(fmt, "diode protocol error: {e}"),
            Self::Other(e) => write!(fmt, "{e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(not(feature = "receive-mmsg"))]
impl From<crossbeam_channel::SendError<raptorq::EncodingPacket>> for Error {
    fn from(_: crossbeam_channel::SendError<raptorq::EncodingPacket>) -> Self {
        Self::SendPackets
    }
}

#[cfg(feature = "receive-mmsg")]
impl From<crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>> for Error {
    fn from(_: crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>) -> Self {
        Self::SendPackets
    }
}

impl From<crossbeam_channel::SendError<Reassembled>> for Error {
    fn from(_: crossbeam_channel::SendError<Reassembled>) -> Self {
        Self::SendBlockPackets
    }
}

impl From<crossbeam_channel::SendError<Option<protocol::Block>>> for Error {
    fn from(_: crossbeam_channel::SendError<Option<protocol::Block>>) -> Self {
        Self::SendBlock
    }
}

impl
    From<
        crossbeam_channel::SendError<(
            protocol::ClientId,
            crossbeam_channel::Receiver<protocol::Block>,
        )>,
    > for Error
{
    fn from(
        _: crossbeam_channel::SendError<(
            protocol::ClientId,
            crossbeam_channel::Receiver<protocol::Block>,
        )>,
    ) -> Self {
        Self::SendClients
    }
}

impl From<crossbeam_channel::RecvError> for Error {
    fn from(e: crossbeam_channel::RecvError) -> Self {
        Self::Receive(e)
    }
}

impl From<crossbeam_channel::RecvTimeoutError> for Error {
    fn from(e: crossbeam_channel::RecvTimeoutError) -> Self {
        Self::ReceiveTimeout(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Protocol(e)
    }
}

enum Reassembled {
    Error,
    Block {
        id: u8,
        packets: Vec<raptorq::EncodingPacket>,
    },
}

/// An instance of this data structure is shared by workers to synchronize them and to access
/// communication channels
pub struct Receiver<ClientNew, ClientEnd> {
    config: Config,
    raptorq: protocol::RaptorQ,
    multiplex_control: semka::Sem,
    #[cfg(not(feature = "receive-mmsg"))]
    to_reblock: crossbeam_channel::Sender<raptorq::EncodingPacket>,
    #[cfg(not(feature = "receive-mmsg"))]
    for_reblock: crossbeam_channel::Receiver<raptorq::EncodingPacket>,
    #[cfg(feature = "receive-mmsg")]
    to_reblock: crossbeam_channel::Sender<Vec<raptorq::EncodingPacket>>,
    #[cfg(feature = "receive-mmsg")]
    for_reblock: crossbeam_channel::Receiver<Vec<raptorq::EncodingPacket>>,
    to_decode: crossbeam_channel::Sender<Reassembled>,
    for_decode: crossbeam_channel::Receiver<Reassembled>,
    to_dispatch: crossbeam_channel::Sender<Option<protocol::Block>>,
    for_dispatch: crossbeam_channel::Receiver<Option<protocol::Block>>,
    to_clients: crossbeam_channel::Sender<(
        protocol::ClientId,
        crossbeam_channel::Receiver<protocol::Block>,
    )>,
    for_clients: crossbeam_channel::Receiver<(
        protocol::ClientId,
        crossbeam_channel::Receiver<protocol::Block>,
    )>,
    client_new: ClientNew,
    client_end: ClientEnd,
}

impl<C, ClientNew, ClientEnd, E> Receiver<ClientNew, ClientEnd>
where
    C: Write + AsRawFd,
    ClientNew: Send + Sync + Fn(protocol::ClientId) -> Result<C, E>,
    ClientEnd: Send + Sync + Fn(C, bool),
    E: Into<Error>,
{
    /// # Errors
    ///
    /// Will return `Err` if `multiplex_control` semaphore
    /// cannot be created.
    pub fn new(
        config: Config,
        raptorq: protocol::RaptorQ,
        client_new: ClientNew,
        client_end: ClientEnd,
    ) -> Result<Self, Error> {
        let multiplex_control = semka::Sem::new(config.max_clients)
            .ok_or(Error::Other("failed to create semaphore".into()))?;

        if config.from_mtu > crate::MAX_MTU {
            return Err(Error::Other(format!(
                "mtu {} is too large (> {})",
                config.from_mtu,
                crate::MAX_MTU
            )));
        }

        let (to_reblock, for_reblock) = crossbeam_channel::unbounded();
        let (to_decode, for_decode) = crossbeam_channel::unbounded();
        let (to_dispatch, for_dispatch) = crossbeam_channel::unbounded();
        let (to_clients, for_clients) = crossbeam_channel::unbounded();

        Ok(Self {
            config,
            raptorq,
            multiplex_control,
            to_reblock,
            for_reblock,
            to_decode,
            for_decode,
            to_dispatch,
            for_dispatch,
            to_clients,
            for_clients,
            client_new,
            client_end,
        })
    }

    /// # Errors
    ///
    /// Will return `Err` if scoped threads cannot spawned.
    #[allow(clippy::too_many_lines)]
    pub fn start<'a>(&'a self, scope: &'a thread::Scope<'a, '_>) -> Result<(), Error> {
        log::info!(
            "max {} simultaneous clients/transfers",
            self.config.max_clients
        );

        log::info!("receive mode is {}", self.config.mode);

        log::info!(
            "reset timeout is {} seconds",
            self.config.reset_timeout.as_secs()
        );

        if let Some(abort_timeout) = self.config.abort_timeout {
            log::info!(
                "connections abort timeout set to {} seconds",
                abort_timeout.as_secs()
            );
        } else {
            log::info!("no connection abort timeout");
        }

        if let Some(hb_interval) = self.config.heartbeat_interval {
            log::info!(
                "heartbeat interval is set to {} seconds",
                hb_interval.as_secs()
            );
        } else {
            log::info!("heartbeat is disabled");
        }

        for i in 0..self.config.max_clients {
            thread::Builder::new()
                .name(format!("client_{i}"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = clients::start(self) {
                        log::error!("fatal client_{i} error: {e}");
                    }
                })?;
        }

        thread::Builder::new()
            .name("dispatch".to_string())
            .spawn_scoped(scope, move || {
                if let Err(e) = dispatch::start(self) {
                    log::error!("fatal dispatch error: {e}");
                }
            })?;

        thread::Builder::new()
            .name("decode".into())
            .spawn_scoped(scope, move || {
                if let Err(e) = decode::start(self) {
                    log::error!("fatal decode error: {e}");
                }
            })?;

        thread::Builder::new()
            .name("reblock".to_string())
            .spawn_scoped(scope, move || {
                if let Err(e) = reblock::start(self) {
                    log::error!("fatal reblock error: {e}");
                }
            })?;

        for port in &self.config.from_ports {
            thread::Builder::new()
                .name(format!("udp_{port}"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = udp::start(self, *port) {
                        log::error!("fatal udp_{port} error: {e}");
                    }
                })?;
        }

        log::info!(
            "RaptorQ block of {} bytes splitted from {}/{} packets",
            self.raptorq.block_size(),
            self.raptorq.min_nb_packets(),
            self.raptorq.nb_packets()
        );

        log::debug!("{}", self.raptorq);

        Ok(())
    }
}
