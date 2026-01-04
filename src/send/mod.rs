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
//!             ----------             ---------               -----------
//! listeners --| client |-> clients --| block |-> encodings --| packets |-> udp
//!             ----------             ---------               -----------
//! ```
//!
//! Notes:
//! - listeners threads are spawned from binary and not the library crate,
//! - heartbeat worker has been omitted from the representation for readability,
//! - there are `max_clients` clients workers running in parallel,
//! - there are `nb_encode_threads` encoding workers running in parallel.

use crate::protocol;
use std::{
    fmt,
    io::{self, Read},
    iter, net,
    os::fd::AsRawFd,
    sync, thread, time,
};

mod client;
mod encoding;
mod heartbeat;
mod server;
mod udp;

pub struct Config {
    pub max_clients: protocol::ClientId,
    pub flush: bool,
    pub nb_encode_threads: u8,
    pub heartbeat_interval: Option<time::Duration>,
    pub to: net::SocketAddr,
    pub to_bind: net::SocketAddr,
    pub to_mtu: u16,
    pub batch_send: Option<u32>,
    pub cpu_affinity: bool,
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

impl From<crossbeam_channel::SendError<protocol::Block>> for Error {
    fn from(_: crossbeam_channel::SendError<protocol::Block>) -> Self {
        Self::SendBlock
    }
}

impl From<crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>> for Error {
    fn from(_: crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>) -> Self {
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
    block_to_encode: sync::Mutex<u8>,
    block_to_send: sync::Mutex<u8>,
    to_server: crossbeam_channel::Sender<C>,
    for_server: crossbeam_channel::Receiver<C>,
    to_encoding: crossbeam_channel::Sender<protocol::Block>,
    for_encoding: crossbeam_channel::Receiver<protocol::Block>,
    to_send: crossbeam_channel::Sender<Vec<raptorq::EncodingPacket>>,
    for_send: crossbeam_channel::Receiver<Vec<raptorq::EncodingPacket>>,
}

impl<C> Sender<C>
where
    C: Read + AsRawFd + Send,
{
    pub fn new(config: Config, raptorq: protocol::RaptorQ) -> Result<Self, Error> {
        let multiplex_control = semka::Sem::new(config.max_clients)
            .ok_or(Error::Other("failed to create semaphore".into()))?;

        let block_to_encode = sync::Mutex::new(0);

        let block_to_send = sync::Mutex::new(0);

        let (to_server, for_server) = crossbeam_channel::bounded(1);
        let (to_encoding, for_encoding) =
            crossbeam_channel::bounded(config.nb_encode_threads as usize);
        let (to_send, for_send) = crossbeam_channel::bounded(config.nb_encode_threads as usize);

        Ok(Self {
            config,
            raptorq,
            multiplex_control,
            block_to_encode,
            block_to_send,
            to_server,
            for_server,
            to_encoding,
            for_encoding,
            to_send,
            for_send,
        })
    }

    pub fn start<'a>(&'a self, scope: &'a thread::Scope<'a, '_>) -> Result<(), Error> {
        log::info!(
            "max {} simultaneous clients/transfers",
            self.config.max_clients
        );

        if let Some(batch) = self.config.batch_send.as_ref() {
            log::info!("batch send {batch} packets");

            let nb_packets = self.raptorq.nb_packets();
            if *batch < nb_packets {
                log::warn!("batch size ({batch} packets) < {nb_packets}");
            }
        }

        let mut cpu_ids = if self.config.cpu_affinity {
            core_affinity::get_core_ids().map(iter::IntoIterator::into_iter)
        } else {
            None
        };

        let cpu_id = cpu_ids.as_mut().and_then(iter::Iterator::next);
        thread::Builder::new()
            .name("udp".into())
            .spawn_scoped(scope, move || {
                if let Some(cpu_id) = cpu_id {
                    log::debug!("set CPU affinity to {}", cpu_id.id);
                    core_affinity::set_for_current(cpu_id);
                }
                if let Err(e) = udp::start(self) {
                    log::error!("fatal udp error: {e}");
                }
            })?;

        for i in 0..self.config.nb_encode_threads {
            let cpu_id = cpu_ids.as_mut().and_then(iter::Iterator::next);
            thread::Builder::new()
                .name(format!("encoding_{i}"))
                .spawn_scoped(scope, move || {
                    if let Some(cpu_id) = cpu_id {
                        log::debug!("set CPU affinity to {}", cpu_id.id);
                        core_affinity::set_for_current(cpu_id);
                    }
                    if let Err(e) = encoding::start(self) {
                        log::error!("fatal encoding_{i} error: {e}");
                    }
                })?;
        }

        if let Some(hb_interval) = self.config.heartbeat_interval {
            log::info!(
                "heartbeat block will be sent every {} seconds",
                hb_interval.as_secs()
            );
            let cpu_id = cpu_ids.as_mut().and_then(iter::Iterator::next);
            thread::Builder::new()
                .name("heartbeat".into())
                .spawn_scoped(scope, move || {
                    if let Some(cpu_id) = cpu_id {
                        log::debug!("set CPU affinity to {}", cpu_id.id);
                        core_affinity::set_for_current(cpu_id);
                    }
                    if let Err(e) = heartbeat::start(self) {
                        log::error!("fatal heartbeat error; {e}");
                    }
                })?;
        } else {
            log::info!("heartbeat is disabled");
        }

        for i in 0..self.config.max_clients {
            let cpu_id = cpu_ids.as_mut().and_then(iter::Iterator::next);
            thread::Builder::new()
                .name(format!("client_{i}"))
                .spawn_scoped(scope, move || {
                    if let Some(cpu_id) = cpu_id {
                        log::debug!("set CPU affinity to {}", cpu_id.id);
                        core_affinity::set_for_current(cpu_id);
                    }
                    if let Err(e) = server::start(self) {
                        log::error!("fatal client_{i} error: {e}");
                    }
                })?;
        }

        log::info!(
            "RaptorQ block contains from {} to {} packets",
            self.raptorq.min_nb_packets(),
            self.raptorq.nb_packets()
        );

        log::debug!("{}", self.raptorq);

        Ok(())
    }

    pub fn new_client(&self, client: C) -> Result<(), Error> {
        if let Err(e) = self.to_server.send(client) {
            return Err(Error::Diode(format!("failed to enqueue client: {e}")));
        }
        Ok(())
    }
}
