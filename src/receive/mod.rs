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
    iter, net,
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
    pub from: net::SocketAddr,
    pub from_mtu: u16,
    pub batch_receive: Option<u32>,
    pub reset_timeout: time::Duration,
    pub nb_decode_threads: u8,
    pub max_clients: protocol::ClientId,
    pub flush: bool,
    pub abort_timeout: Option<time::Duration>,
    pub heartbeat_interval: Option<time::Duration>,
    pub cpu_affinity: bool,
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

impl From<crossbeam_channel::SendError<crate::udp::Datagrams>> for Error {
    fn from(_: crossbeam_channel::SendError<crate::udp::Datagrams>) -> Self {
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
pub struct Receiver<F> {
    config: Config,
    raptorq: protocol::RaptorQ,
    multiplex_control: semka::Sem,
    to_reblock: crossbeam_channel::Sender<crate::udp::Datagrams>,
    for_reblock: crossbeam_channel::Receiver<crate::udp::Datagrams>,
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
    new_client: F,
}

impl<C, F, E> Receiver<F>
where
    C: Write + AsRawFd,
    F: Send + Sync + Fn() -> Result<C, E>,
    E: Into<Error>,
{
    pub fn new(config: Config, raptorq: protocol::RaptorQ, new_client: F) -> Result<Self, Error> {
        let multiplex_control = semka::Sem::new(config.max_clients)
            .ok_or(Error::Other("failed to create semaphore".into()))?;

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
            new_client,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub fn start<'a>(&'a self, scope: &'a thread::Scope<'a, '_>) -> Result<(), Error> {
        log::info!(
            "max {} simultaneous clients/transfers",
            self.config.max_clients
        );

        if let Some(batch) = self.config.batch_receive.as_ref() {
            log::info!("batch receive {batch} packets");

            let nb_packets = self.raptorq.nb_packets();
            if *batch < nb_packets {
                log::warn!("batch size ({batch} packets) < {nb_packets}");
            }
        }

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

        let mut cpu_ids = if self.config.cpu_affinity {
            core_affinity::get_core_ids().map(|ids| ids.into_iter().rev())
        } else {
            None
        };

        for i in 0..self.config.max_clients {
            let cpu_id = cpu_ids.as_mut().and_then(iter::Iterator::next);
            thread::Builder::new()
                .name(format!("client_{i}"))
                .spawn_scoped(scope, move || {
                    if let Some(cpu_id) = cpu_id {
                        log::debug!("set CPU affinity to {}", cpu_id.id);
                        core_affinity::set_for_current(cpu_id);
                    }
                    if let Err(e) = clients::start(self) {
                        log::error!("fatal client_{i} error: {e}");
                    }
                })?;
        }

        let cpu_id = cpu_ids.as_mut().and_then(iter::Iterator::next);
        thread::Builder::new()
            .name("dispatch".to_string())
            .spawn_scoped(scope, move || {
                if let Some(cpu_id) = cpu_id {
                    log::debug!("set CPU affinity to {}", cpu_id.id);
                    core_affinity::set_for_current(cpu_id);
                }
                if let Err(e) = dispatch::start(self) {
                    log::error!("fatal dispatch error: {e}");
                }
            })?;

        let cpu_id = cpu_ids.as_mut().and_then(iter::Iterator::next);
        for i in 0..self.config.nb_decode_threads {
            thread::Builder::new()
                .name(format!("decode_{i}"))
                .spawn_scoped(scope, move || {
                    if let Some(cpu_id) = cpu_id {
                        log::debug!("set CPU affinity to {}", cpu_id.id);
                        core_affinity::set_for_current(cpu_id);
                    }
                    if let Err(e) = decode::start(self) {
                        log::error!("fatal decode_{i} error: {e}");
                    }
                })?;
        }

        let cpu_id = cpu_ids.as_mut().and_then(iter::Iterator::next);
        thread::Builder::new()
            .name("reblock".to_string())
            .spawn_scoped(scope, move || {
                if let Some(cpu_id) = cpu_id {
                    log::debug!("set CPU affinity to {}", cpu_id.id);
                    core_affinity::set_for_current(cpu_id);
                }
                if let Err(e) = reblock::start(self) {
                    log::error!("fatal reblock error: {e}");
                }
            })?;

        let cpu_id = cpu_ids.as_mut().and_then(iter::Iterator::next);
        thread::Builder::new()
            .name("udp".to_string())
            .spawn_scoped(scope, move || {
                if let Some(cpu_id) = cpu_id {
                    log::debug!("set CPU affinity to {}", cpu_id.id);
                    core_affinity::set_for_current(cpu_id);
                }
                if let Err(e) = udp::start(self) {
                    log::error!("fatal udp error: {e}");
                }
            })?;

        log::info!(
            "RaptorQ block contains from {} to {} packets",
            self.raptorq.min_nb_packets(),
            self.raptorq.nb_packets()
        );

        log::debug!("{}", self.raptorq);

        Ok(())
    }
}
