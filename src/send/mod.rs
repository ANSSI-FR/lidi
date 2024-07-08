//! Sender functions module
//!
//! Several threads are used to form a pipeline for the data to be prepared before sending it over
//! UDP. Every submodule of the [crate::send] module is equipped with a `start` function that
//! launch the worker process. Data pass through the workers pipelines via [crossbeam_channel]
//! bounded channels.
//!
//! Here follows a simplified representation of the workers pipeline:
//!
//! ```text
//!             ----------             ------------               -----------
//! listeners --| client |-> clients --| messages |-> encodings --| packets |-> udp
//!             ----------             ------------               -----------
//! ```
//!
//! Notes:
//! - listeners threads are spawned from binary and not the library crate,
//! - heartbeat worker has been omitted from the representation for readability,
//! - there are `nb_clients` clients workers running in parallel,
//! - there are `nb_encoding_threads` encoding workers running in parallel.

use crate::{protocol, semaphore};
use std::{
    fmt,
    io::{self, Read},
    net,
    os::fd::AsRawFd,
    sync, thread, time,
};

mod client;
mod encoding;
mod heartbeat;
mod server;
mod udp;

pub struct Config {
    pub nb_clients: u16,
    pub encoding_block_size: u64,
    pub repair_block_size: u32,
    pub udp_buffer_size: u32,
    pub nb_encoding_threads: u8,
    pub heartbeat_interval: Option<time::Duration>,
    pub to_bind: net::SocketAddr,
    pub to_udp: net::SocketAddr,
    pub to_mtu: u16,
    pub bandwidth_limit: f64,
}

impl Config {
    pub(crate) fn adjust(&mut self) {
        let oti = protocol::object_transmission_information(self.to_mtu, self.encoding_block_size);

        let packet_size = protocol::packet_size(&oti);
        let nb_encoding_packets = protocol::nb_encoding_packets(&oti);
        let nb_repair_packets = protocol::nb_repair_packets(&oti, self.repair_block_size);

        self.encoding_block_size = nb_encoding_packets * u64::from(packet_size);
        self.repair_block_size = nb_repair_packets * u32::from(packet_size);
    }
}

pub enum Error {
    Io(io::Error),
    SendMessage(crossbeam_channel::SendError<protocol::Message>),
    SendUdp(crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>),
    Receive(crossbeam_channel::RecvError),
    Protocol(protocol::Error),
    Diode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::SendMessage(e) => write!(fmt, "crossbeam send message error: {e}"),
            Self::SendUdp(e) => write!(fmt, "crossbeam send UDP error: {e}"),
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::Protocol(e) => write!(fmt, "diode protocol error: {e}"),
            Self::Diode(e) => write!(fmt, "diode error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crossbeam_channel::SendError<protocol::Message>> for Error {
    fn from(e: crossbeam_channel::SendError<protocol::Message>) -> Self {
        Self::SendMessage(e)
    }
}

impl From<crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>> for Error {
    fn from(e: crossbeam_channel::SendError<Vec<raptorq::EncodingPacket>>) -> Self {
        Self::SendUdp(e)
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
    pub(crate) config: Config,
    pub(crate) object_transmission_info: raptorq::ObjectTransmissionInformation,
    pub(crate) from_buffer_size: u32,
    pub(crate) to_max_messages: u16,
    pub(crate) multiplex_control: semaphore::Semaphore,
    pub(crate) block_to_encode: sync::Mutex<u8>,
    pub(crate) block_to_send: sync::Mutex<u8>,
    pub(crate) to_server: crossbeam_channel::Sender<C>,
    pub(crate) for_server: crossbeam_channel::Receiver<C>,
    pub(crate) to_encoding: crossbeam_channel::Sender<protocol::Message>,
    pub(crate) for_encoding: crossbeam_channel::Receiver<protocol::Message>,
    pub(crate) to_send: crossbeam_channel::Sender<Vec<raptorq::EncodingPacket>>,
    pub(crate) for_send: crossbeam_channel::Receiver<Vec<raptorq::EncodingPacket>>,
}

impl<C> Sender<C>
where
    C: Read + AsRawFd + Send,
{
    pub fn new(mut config: Config) -> Self {
        config.adjust();

        let object_transmission_info =
            protocol::object_transmission_information(config.to_mtu, config.encoding_block_size);

        let from_buffer_size = (object_transmission_info.transfer_length()
            - protocol::Message::serialize_overhead() as u64) as u32;

        let to_max_messages = protocol::nb_encoding_packets(&object_transmission_info) as u16
            + protocol::nb_repair_packets(&object_transmission_info, config.repair_block_size)
                as u16;

        let multiplex_control = semaphore::Semaphore::new(config.nb_clients as usize);

        let block_to_encode = sync::Mutex::new(0);

        let block_to_send = sync::Mutex::new(0);

        let (to_server, for_server) = crossbeam_channel::bounded::<C>(1);

        let (to_encoding, for_encoding) =
            crossbeam_channel::bounded::<protocol::Message>(config.nb_clients as usize);

        let (to_send, for_send) = crossbeam_channel::bounded::<Vec<raptorq::EncodingPacket>>(
            2 * config.nb_encoding_threads as usize,
        );

        Self {
            config,
            object_transmission_info,
            from_buffer_size,
            to_max_messages,
            multiplex_control,
            block_to_encode,
            block_to_send,
            to_server,
            for_server,
            to_encoding,
            for_encoding,
            to_send,
            for_send,
        }
    }

    pub fn start<'a>(&'a self, scope: &'a thread::Scope<'a, '_>) -> Result<(), Error> {
        log::info!(
            "accepting {} simultaneous transfers",
            self.config.nb_clients
        );

        log::info!(
            "client socket buffer size is {} bytes",
            self.from_buffer_size
        );

        log::info!(
            "encoding will produce {} packets ({} bytes per block) + {} repair packets",
            protocol::nb_encoding_packets(&self.object_transmission_info),
            self.config.encoding_block_size,
            protocol::nb_repair_packets(
                &self.object_transmission_info,
                self.config.repair_block_size
            ),
        );

        thread::Builder::new()
            .name("udp".into())
            .spawn_scoped(scope, || udp::start(self))?;

        for i in 0..self.config.nb_encoding_threads {
            thread::Builder::new()
                .name(format!("encoding_{i}"))
                .spawn_scoped(scope, || encoding::start(self))?;
        }

        if let Some(hb_interval) = self.config.heartbeat_interval {
            log::info!(
                "heartbeat message will be sent every {} seconds",
                hb_interval.as_secs()
            );
            thread::Builder::new()
                .name("heartbeat".into())
                .spawn_scoped(scope, || heartbeat::start(self))?;
        } else {
            log::info!("heartbeat is disabled");
        }

        for i in 0..self.config.nb_clients {
            thread::Builder::new()
                .name(format!("client_{i}"))
                .spawn_scoped(scope, || server::start(self))?;
        }

        Ok(())
    }

    pub fn new_client(&self, client: C) -> Result<(), Error> {
        if let Err(e) = self.to_server.send(client) {
            return Err(Error::Diode(format!("failed to enqueue client: {e}")));
        }
        Ok(())
    }
}
