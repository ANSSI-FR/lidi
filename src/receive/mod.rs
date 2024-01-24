//! Receiver functions module
//!
//! Several threads are involved in the receipt pipeline. Each worker is run with a `start`
//! function of a submodule of the [crate::receive] module, data being passed through
//! [crossbeam_channel] bounded channels to form the following data pipeline:
//!
//! ```text
//!       -----------             ------------------               ------------
//! udp --| packets |-> reblock --| vec of packets |-> decodings --| messages |-> dispatch
//!       -----------             ------------------               ------------
//! ```
//!
//! Notes:
//! - heartbeat does not need a dedicated worker on the receiver side, heartbeat messages are
//! handled by the dispatch worker,
//! - there are `nb_clients` clients workers running in parallel,
//! - there are `nb_decoding_threads` decoding workers running in parallel.

use crate::{protocol, semaphore};
use std::{
    fmt,
    io::{self, Write},
    net,
    os::fd::AsRawFd,
    sync, thread, time,
};

mod client;
mod clients;
mod decoding;
mod dispatch;
mod reblock;
mod udp;

pub struct Config {
    pub from_udp: net::SocketAddr,
    pub from_udp_mtu: u16,
    pub nb_clients: u16,
    pub encoding_block_size: u64,
    pub repair_block_size: u32,
    pub udp_buffer_size: u32,
    pub reblock_retention_window: u8,
    pub flush_timeout: time::Duration,
    pub nb_decoding_threads: u8,
    pub heartbeat_interval: Option<time::Duration>,
}

impl Config {
    pub(crate) fn adjust(&mut self) {
        let oti =
            protocol::object_transmission_information(self.from_udp_mtu, self.encoding_block_size);

        let packet_size = protocol::packet_size(&oti);
        let nb_encoding_packets = protocol::nb_encoding_packets(&oti);
        let nb_repair_packets = protocol::nb_repair_packets(&oti, self.repair_block_size);

        self.encoding_block_size = nb_encoding_packets * u64::from(packet_size);
        self.repair_block_size = nb_repair_packets * u32::from(packet_size);
    }
}

pub enum Error {
    Io(io::Error),
    SendPackets(crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>),
    SendBlockPackets(crossbeam_channel::SendError<(u8, Vec<raptorq::EncodingPacket>)>),
    SendMessage(crossbeam_channel::SendError<protocol::Message>),
    SendClients(
        crossbeam_channel::SendError<(
            protocol::ClientId,
            crossbeam_channel::Receiver<protocol::Message>,
        )>,
    ),
    Receive(crossbeam_channel::RecvError),
    ReceiveTimeout(crossbeam_channel::RecvTimeoutError),
    Protocol(protocol::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::SendPackets(e) => write!(fmt, "crossbeam send packets error: {e}"),
            Self::SendBlockPackets(e) => write!(fmt, "crossbeam send block packets error: {e}"),
            Self::SendMessage(e) => write!(fmt, "crossbeam send message error: {e}"),
            Self::SendClients(e) => write!(fmt, "crossbeam send client error: {e}"),
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::ReceiveTimeout(e) => write!(fmt, "crossbeam receive timeout error: {e}"),
            Self::Protocol(e) => write!(fmt, "diode protocol error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>> for Error {
    fn from(e: crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>) -> Self {
        Self::SendPackets(e)
    }
}

impl From<crossbeam_channel::SendError<(u8, Vec<raptorq::EncodingPacket>)>> for Error {
    fn from(e: crossbeam_channel::SendError<(u8, Vec<raptorq::EncodingPacket>)>) -> Self {
        Self::SendBlockPackets(e)
    }
}

impl From<crossbeam_channel::SendError<protocol::Message>> for Error {
    fn from(e: crossbeam_channel::SendError<protocol::Message>) -> Self {
        Self::SendMessage(e)
    }
}

impl
    From<
        crossbeam_channel::SendError<(
            protocol::ClientId,
            crossbeam_channel::Receiver<protocol::Message>,
        )>,
    > for Error
{
    fn from(
        e: crossbeam_channel::SendError<(
            protocol::ClientId,
            crossbeam_channel::Receiver<protocol::Message>,
        )>,
    ) -> Self {
        Self::SendClients(e)
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

/// An instance of this data structure is shared by workers to synchronize them and to access
/// communication channels
pub struct Receiver<F> {
    pub(crate) config: Config,
    pub(crate) object_transmission_info: raptorq::ObjectTransmissionInformation,
    pub(crate) to_buffer_size: usize,
    pub(crate) from_max_messages: u16,
    pub(crate) multiplex_control: semaphore::Semaphore,
    pub(crate) block_to_receive: sync::Mutex<u8>,
    pub(crate) to_reblock: crossbeam_channel::Sender<Vec<raptorq::EncodingPacket>>,
    pub(crate) for_reblock: crossbeam_channel::Receiver<Vec<raptorq::EncodingPacket>>,
    pub(crate) to_decoding: crossbeam_channel::Sender<(u8, Vec<raptorq::EncodingPacket>)>,
    pub(crate) for_decoding: crossbeam_channel::Receiver<(u8, Vec<raptorq::EncodingPacket>)>,
    pub(crate) to_dispatch: crossbeam_channel::Sender<protocol::Message>,
    pub(crate) for_dispatch: crossbeam_channel::Receiver<protocol::Message>,
    pub(crate) to_clients: crossbeam_channel::Sender<(
        protocol::ClientId,
        crossbeam_channel::Receiver<protocol::Message>,
    )>,
    pub(crate) for_clients: crossbeam_channel::Receiver<(
        protocol::ClientId,
        crossbeam_channel::Receiver<protocol::Message>,
    )>,
    pub(crate) new_client: F,
}

impl<C, F, E> Receiver<F>
where
    C: Write + AsRawFd,
    F: Send + Sync + Fn() -> Result<C, E>,
    E: Into<Error>,
{
    pub fn new(mut config: Config, new_client: F) -> Self {
        config.adjust();

        let object_transmission_info = protocol::object_transmission_information(
            config.from_udp_mtu,
            config.encoding_block_size,
        );

        let to_buffer_size =
            config.encoding_block_size as usize - protocol::Message::serialize_overhead();

        let from_max_messages = protocol::nb_encoding_packets(&object_transmission_info) as u16
            + protocol::nb_repair_packets(&object_transmission_info, config.repair_block_size)
                as u16;

        let multiplex_control = semaphore::Semaphore::new(config.nb_clients as usize);

        let block_to_receive = sync::Mutex::new(0);

        let (to_reblock, for_reblock) =
            crossbeam_channel::unbounded::<Vec<raptorq::EncodingPacket>>();
        let (to_decoding, for_decoding) =
            crossbeam_channel::unbounded::<(u8, Vec<raptorq::EncodingPacket>)>();
        let (to_dispatch, for_dispatch) = crossbeam_channel::unbounded::<protocol::Message>();

        let (to_clients, for_clients) = crossbeam_channel::bounded::<(
            protocol::ClientId,
            crossbeam_channel::Receiver<protocol::Message>,
        )>(1);

        Self {
            config,
            object_transmission_info,
            to_buffer_size,
            from_max_messages,
            multiplex_control,
            block_to_receive,
            to_reblock,
            for_reblock,
            to_decoding,
            for_decoding,
            to_dispatch,
            for_dispatch,
            to_clients,
            for_clients,
            new_client,
        }
    }

    pub fn start<'a>(&'a self, scope: &'a thread::Scope<'a, '_>) -> Result<(), Error> {
        log::info!(
            "accepting {} simultaneous transfers",
            self.config.nb_clients
        );

        log::info!("client socket buffer size is {} bytes", self.to_buffer_size);

        log::info!(
            "decoding will expect {} packets ({} bytes per block) + {} repair packets",
            protocol::nb_encoding_packets(&self.object_transmission_info),
            self.config.encoding_block_size,
            protocol::nb_repair_packets(
                &self.object_transmission_info,
                self.config.repair_block_size
            ),
        );

        log::info!(
            "flush timeout is {} ms",
            self.config.flush_timeout.as_millis()
        );

        if let Some(hb_interval) = self.config.heartbeat_interval {
            log::info!(
                "heartbeat interval is set to {} seconds",
                hb_interval.as_secs()
            );
        } else {
            log::info!("heartbeat is disabled");
        }

        for i in 0..self.config.nb_clients {
            thread::Builder::new()
                .name(format!("client_{i}"))
                .spawn_scoped(scope, || clients::start(self))?;
        }

        thread::Builder::new()
            .name("dispatch".to_string())
            .spawn_scoped(scope, || dispatch::start(self))?;

        for i in 0..self.config.nb_decoding_threads {
            thread::Builder::new()
                .name(format!("decoding_{i}"))
                .spawn_scoped(scope, || decoding::start(self))?;
        }

        thread::Builder::new()
            .name("reblock".to_string())
            .spawn_scoped(scope, || reblock::start(self))?;

        thread::Builder::new()
            .name("udp".to_string())
            .spawn_scoped(scope, || udp::start(self))?;

        Ok(())
    }
}
