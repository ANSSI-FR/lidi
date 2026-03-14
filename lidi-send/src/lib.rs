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

use lidi_command_utils::config;
#[cfg(feature = "from-tls")]
use lidi_command_utils::tls;
use lidi_protocol as protocol;
#[cfg(any(feature = "heartbeat", feature = "prometheus"))]
use std::time;
use std::{
    fmt,
    io::{self, Read},
    net,
    os::fd::AsRawFd,
    sync, thread,
};

mod client;
#[cfg(feature = "heartbeat")]
mod heartbeat;
mod server;
mod socket;
mod udp;

pub enum Error {
    Io(io::Error),
    SendToUdp,
    Receive(crossbeam_channel::RecvError),
    Protocol(protocol::Error),
    Internal(String),
    #[cfg(feature = "from-tls")]
    Tls(tls::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::SendToUdp => write!(fmt, "crossbeam send to UDP error"),
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::Protocol(e) => write!(fmt, "diode protocol error: {e}"),
            Self::Internal(e) => write!(fmt, "internal error: {e}"),
            #[cfg(feature = "from-tls")]
            Self::Tls(e) => write!(fmt, "TLS error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crossbeam_channel::SendError<Option<(u8, protocol::Block)>>> for Error {
    fn from(_: crossbeam_channel::SendError<Option<(u8, protocol::Block)>>) -> Self {
        Self::SendToUdp
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

#[cfg(feature = "from-tls")]
impl From<tls::Error> for Error {
    fn from(e: tls::Error) -> Self {
        Self::Tls(e)
    }
}

struct Config {
    mtu: u16,
    ports: Vec<u16>,
    max_clients: u32,
    #[cfg(feature = "hash")]
    hash: bool,
    flush: bool,
    #[cfg(feature = "heartbeat")]
    heartbeat: Option<time::Duration>,
    to: net::IpAddr,
    to_bind: net::SocketAddr,
    mode: config::Mode,
    #[cfg(feature = "from-tls")]
    tls: config::TlsConfig,
    #[cfg(feature = "prometheus")]
    prometheus_listen: Option<net::SocketAddr>,
}

impl From<&config::Config> for Config {
    fn from(config: &config::Config) -> Self {
        let common = config.common();
        let send = config.send();

        #[cfg(not(feature = "hash"))]
        if common.hash() {
            log::warn!("hash was not enabled at compilation, ignoring this parameter");
        }

        #[cfg(not(feature = "heartbeat"))]
        if common.heartbeat().is_some() {
            log::warn!("heartbeat was not enabled at compilation, ignoring this parameter");
        }

        let available_modes = [
            #[cfg(feature = "send-mmsg")]
            config::Mode::Mmsg,
            #[cfg(feature = "send-msg")]
            config::Mode::Msg,
            #[cfg(feature = "send-native")]
            config::Mode::Native,
        ];

        let mode = send
            .mode()
            .filter(|mode| {
                if available_modes.contains(mode) {
                    true
                } else {
                    log::warn!("mode {mode} was not enabled at compilation");
                    false
                }
            })
            .unwrap_or_else(|| available_modes[0]);

        Self {
            mtu: common.mtu(),
            ports: common.ports(),
            max_clients: common.max_clients(),
            #[cfg(feature = "hash")]
            hash: common.hash(),
            flush: common.flush(),
            #[cfg(feature = "heartbeat")]
            heartbeat: common.heartbeat(),
            to: send.to(),
            to_bind: send.to_bind(),
            mode,
            #[cfg(feature = "from-tls")]
            tls: send.tls(),
            #[cfg(feature = "prometheus")]
            prometheus_listen: config.send().prometheus_listen(),
        }
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
    next_block: sync::atomic::AtomicU8,
    to_server: crossbeam_channel::Sender<Option<(protocol::EndpointId, C)>>,
    for_server: crossbeam_channel::Receiver<Option<(protocol::EndpointId, C)>>,
    to_udp: crossbeam_channel::Sender<Option<(u8, protocol::Block)>>,
    for_udp: crossbeam_channel::Receiver<Option<(u8, protocol::Block)>>,
}

impl<C> Sender<C>
where
    C: Read + AsRawFd + Send,
{
    #[cfg(feature = "prometheus")]
    #[allow(clippy::cast_precision_loss)]
    fn metrics_loop(&self) {
        let timer = time::Duration::from_secs(1);

        loop {
            thread::sleep(timer);

            metrics::gauge!("lidi_send_udp_queue_len").set(self.for_udp.len() as f64);
        }
    }

    pub fn new(config: &config::Config, raptorq: protocol::RaptorQ) -> Result<Self, Error> {
        let config = Config::from(config);

        if config.ports.is_empty() {
            return Err(Error::Internal(String::from("no ports configured")));
        }

        let next_block = sync::atomic::AtomicU8::new(0);
        let (to_server, for_server) = crossbeam_channel::bounded(1);
        let (to_udp, for_udp) = crossbeam_channel::bounded(config.ports.len());

        Ok(Self {
            config,
            raptorq,
            next_block,
            to_server,
            for_server,
            to_udp,
            for_udp,
        })
    }

    #[cfg(feature = "from-tls")]
    pub const fn tls(&self) -> &config::TlsConfig {
        &self.config.tls
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

        for port in &self.config.ports {
            thread::Builder::new()
                .name(format!("udp_{port}"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = udp::start(self, *port) {
                        log::error!("fatal udp error: {e}");
                    }
                })?;
        }

        #[cfg(feature = "heartbeat")]
        if let Some(hb_interval) = self.config.heartbeat {
            log::info!(
                "heartbeat block will be sent every {} seconds",
                hb_interval.as_secs()
            );
            thread::Builder::new()
                .name(String::from("heartbeat"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = heartbeat::start(self) {
                        log::error!("fatal heartbeat error; {e}");
                    }
                })?;
        } else {
            log::info!("heartbeat is disabled");
        }

        #[cfg(feature = "prometheus")]
        if let Some(prometheus) = self.config.prometheus_listen {
            log::info!("Prometheus is set to {prometheus}");

            thread::Builder::new()
                .name(String::from("metrics"))
                .spawn_scoped(scope, move || {
                    self.metrics_loop();
                })?;
        } else {
            log::info!("Prometheus is disabled");
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
            "RaptorQ block {} bytes in {} packets + {} repair packets + {} spare packets",
            self.raptorq.block_size(),
            self.raptorq.min_nb_packets() - protocol::MIN_NB_REPAIR_PACKETS,
            protocol::MIN_NB_REPAIR_PACKETS,
            self.raptorq.nb_packets() - u32::from(self.raptorq.min_nb_packets()),
        );

        log::debug!("{}", self.raptorq);

        Ok(())
    }

    /// # Errors
    ///
    /// Will return `Err` if the `send` returns a `SendError<T>`.
    pub fn new_client(&self, endpoint: protocol::EndpointId, client: C) -> Result<(), Error> {
        if let Err(e) = self.to_server.send(Some((endpoint, client))) {
            return Err(Error::Internal(format!("failed to enqueue client: {e}")));
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Will return `Err` if the `send` returns a `SendError<T>`.
    pub fn stop(&self) -> Result<(), Error> {
        if let Err(e) = self.to_server.send(None) {
            return Err(Error::Internal(format!("failed to stop: {e}")));
        }
        Ok(())
    }
}
