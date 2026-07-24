//! Receiver functions module
//!
//! Several threads are involved in the receipt pipeline. Each worker is run with a `start`
//! function of a submodule of this crate ([`crate`]), data being passed through
//! [`crossbeam_channel`] bounded channels to form the following data pipeline:
//!
//! ```text
//!       -----------                              ---------
//! udp --| packets |-> reblock (RaptorQ decode) --| block |-> dispatch -> clients
//!       -----------                              ---------
//! ```
//!
//! The `reblock` worker both regroups the received packets into `RaptorQ` blocks and decodes them;
//! there is no separate decode worker. The `dispatch` worker then routes each decoded block to the
//! `clients` worker handling the matching client, which writes it to the output endpoint.
//!
//! Notes:
//! - heartbeat does not need a dedicated worker on the receiver side, heartbeat blocks are
//!   handled by the dispatch worker,
//! - there are `max_clients` clients workers running in parallel,
//! - there is one `udp` (recv) worker and one `reblock` worker per configured UDP port.

#[cfg(not(any(
    feature = "receive-native",
    feature = "receive-msg",
    feature = "receive-mmsg"
)))]
compile_error!(
    "at least one of receive-native, receive-msg, or receive-mmsg features must be enabled"
);

use lidi_command_utils::config;
#[cfg(feature = "to-tls")]
use lidi_command_utils::tls;
use lidi_protocol as protocol;
#[cfg(any(feature = "to-tcp", feature = "prometheus"))]
use std::net;
#[cfg(feature = "to-unix")]
use std::os::unix;
use std::{fmt, io, os, thread, time};

mod client;
mod client_reorder;
mod clients;
mod dispatch;
mod reblock;
mod socket;
mod udp;

/// Errors returned by the receiver engine and its workers.
pub enum Error {
    /// An underlying I/O operation failed.
    Io(io::Error),
    /// Sending received packets to the reblock worker failed.
    SendPackets,
    /// Sending a block's packets between workers failed.
    SendBlockPackets,
    /// Sending a decoded block to the dispatch worker failed.
    SendBlock,
    /// Sending a client to a client worker failed.
    SendClients,
    /// Receiving from an internal worker channel failed.
    Receive(crossbeam_channel::RecvError),
    /// Receiving from an internal worker channel timed out.
    ReceiveTimeout(crossbeam_channel::RecvTimeoutError),
    /// A protocol-level (block decoding) error occurred.
    Protocol(protocol::Error),
    /// An internal invariant was violated.
    Internal(String),
    /// A TLS error occurred.
    #[cfg(feature = "to-tls")]
    Tls(lidi_command_utils::tls::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::SendPackets => write!(fmt, "crossbeam send packets error"),
            Self::SendBlockPackets => write!(fmt, "crossbeam send block packets error"),
            Self::SendBlock => write!(fmt, "crossbeam send block error"),
            Self::SendClients => write!(fmt, "crossbeam send client error"),
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::ReceiveTimeout(e) => write!(fmt, "crossbeam receive timeout error: {e}"),
            Self::Protocol(e) => write!(fmt, "diode protocol error: {e}"),
            Self::Internal(e) => write!(fmt, "internal error: {e}"),
            #[cfg(feature = "to-tls")]
            Self::Tls(e) => write!(fmt, "TLS error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crossbeam_channel::SendError<reblock::Message>> for Error {
    fn from(_: crossbeam_channel::SendError<reblock::Message>) -> Self {
        Self::SendPackets
    }
}

impl From<crossbeam_channel::SendError<dispatch::Message>> for Error {
    fn from(_: crossbeam_channel::SendError<dispatch::Message>) -> Self {
        Self::SendBlock
    }
}

impl From<crossbeam_channel::SendError<protocol::Block>> for Error {
    fn from(_: crossbeam_channel::SendError<protocol::Block>) -> Self {
        Self::SendBlock
    }
}

impl
    From<
        crossbeam_channel::SendError<(
            protocol::EndpointId,
            protocol::ClientId,
            crossbeam_channel::Receiver<protocol::Block>,
        )>,
    > for Error
{
    fn from(
        _: crossbeam_channel::SendError<(
            protocol::EndpointId,
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

#[cfg(feature = "to-tls")]
impl From<tls::Error> for Error {
    fn from(e: tls::Error) -> Self {
        Self::Tls(e)
    }
}

struct Config {
    mtu: u16,
    ports: Vec<u16>,
    max_clients: u32,
    #[cfg(feature = "heartbeat")]
    heartbeat: Option<time::Duration>,
    from: String,
    to: Vec<config::Endpoint>,
    reset_timeout: time::Duration,
    abort_timeout: Option<time::Duration>,
    // Per-client block queue size (0 = unbounded). Matches lidi_receive_client_queue_full.
    client_queue_size: usize,
    // Per-stage pipeline queue sizes (0 = unbounded). Match lidi_receive_*_queue_len metrics.
    reblock_queue_size: usize,
    dispatch_queue_size: usize,
    clients_queue_size: usize,
    mode: config::Mode,
    #[cfg(feature = "prometheus")]
    prometheus_listen: Option<net::SocketAddr>,
}

impl From<&config::ReceiveConfig> for Config {
    fn from(config: &config::ReceiveConfig) -> Self {
        #[cfg(not(feature = "heartbeat"))]
        if config.common.heartbeat().is_some() {
            log::warn!("heartbeat was not enabled at compilation, ignoring this parameter");
        }

        let available_modes = [
            #[cfg(feature = "receive-mmsg")]
            config::Mode::Mmsg,
            #[cfg(feature = "receive-msg")]
            config::Mode::Msg,
            #[cfg(feature = "receive-native")]
            config::Mode::Native,
        ];

        let mode = config
            .receive
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
            mtu: config.common.mtu(),
            ports: config.common.ports(),
            max_clients: config.common.max_clients(),
            #[cfg(feature = "heartbeat")]
            heartbeat: config.common.heartbeat(),
            from: config.receive.from().into(),
            to: config.receive.to(),
            reset_timeout: config.receive.reset_timeout(),
            abort_timeout: config.receive.abort_timeout(),
            client_queue_size: config.receive.client_queue_size(),
            reblock_queue_size: config.receive.reblock_queue_size(),
            dispatch_queue_size: config.receive.dispatch_queue_size(),
            clients_queue_size: config.receive.clients_queue_size(),
            mode,
            #[cfg(feature = "prometheus")]
            prometheus_listen: config.receive.prometheus_listen(),
        }
    }
}

/// The receiver engine driving the receive/decode/dispatch pipeline.
///
/// It holds the shared configuration, `RaptorQ` state, worker channels and the active transfers,
/// and starts all workers when [`Receiver::start`] is called. An instance is shared by all workers
/// to synchronize them and to access the communication channels. The `Lifecycle` type parameter
/// customizes how output clients are opened and closed (see [`ClientLifecycle`]).
pub struct Receiver<Lifecycle>
where
    Lifecycle: ClientLifecycle,
{
    config: Config,
    raptorq: protocol::RaptorQ,
    // Batches drained below are sent back here for the udp worker to reuse, mirroring
    // lidi-send's block_recycler.
    #[cfg(feature = "receive-mmsg")]
    packet_vec_recycler: crossbeam_deque::Injector<Vec<raptorq::EncodingPacket>>,
    to_dispatch: crossbeam_channel::Sender<dispatch::Message>,
    for_dispatch: crossbeam_channel::Receiver<dispatch::Message>,
    to_clients: crossbeam_channel::Sender<(
        protocol::EndpointId,
        protocol::ClientId,
        crossbeam_channel::Receiver<protocol::Block>,
    )>,
    for_clients: crossbeam_channel::Receiver<(
        protocol::EndpointId,
        protocol::ClientId,
        crossbeam_channel::Receiver<protocol::Block>,
    )>,
    active_transfers:
        dashmap::DashMap<protocol::ClientId, crossbeam_channel::Sender<protocol::Block>>,
    failed_transfers: dashmap::DashSet<protocol::ClientId>,
    #[cfg(feature = "prometheus")]
    reblock_queues: std::sync::RwLock<Vec<crossbeam_channel::Receiver<reblock::Message>>>,
    client_lifecycle: Lifecycle,
}

/// A destination a decoded transfer can be written to (a TCP/TLS/Unix stream, or stdout).
pub trait Client: io::Write + os::fd::AsRawFd {}

impl Client for io::Stdout {}

#[cfg(feature = "to-tcp")]
impl Client for net::TcpStream {}

#[cfg(feature = "to-tls")]
impl Client for tls::TcpStream {}

#[cfg(feature = "to-unix")]
impl Client for unix::net::UnixStream {}

/// Strategy for opening and closing the output [`Client`] of each transfer, letting callers
/// customize what a transfer is written to (e.g. a socket, a file, or stdout for the oneshot
/// receiver).
pub trait ClientLifecycle: Send + Sync {
    /// Opens the output client for a new transfer on `endpoint` (identified by `client_id`).
    ///
    /// # Errors
    ///
    /// Will return `Err` if the output client cannot be created (e.g. connection failure).
    fn start(
        &self,
        endpoint: &config::Endpoint,
        client_id: protocol::ClientId,
    ) -> Result<Box<dyn Client>, Error>;

    /// Closes the output `client` at the end of a transfer, `ok` indicating whether the transfer
    /// completed successfully.
    ///
    /// # Errors
    ///
    /// Will return `Err` if finalizing the output client fails.
    fn end(&self, client: Box<dyn Client>, ok: bool) -> Result<(), Error>;
}

impl<Lifecycle> Receiver<Lifecycle>
where
    Lifecycle: ClientLifecycle,
{
    #[cfg(feature = "prometheus")]
    #[allow(clippy::cast_precision_loss)]
    fn metrics_loop(&self) {
        let timer = time::Duration::from_secs(1);

        loop {
            thread::sleep(timer);

            if let Ok(queues) = self.reblock_queues.read() {
                let reblock_total: usize =
                    queues.iter().map(crossbeam_channel::Receiver::len).sum();
                log::debug!(
                    "Reblock queue metric: {} queues, total len = {}",
                    queues.len(),
                    reblock_total
                );
                metrics::gauge!("lidi_receive_reblock_queue_len").set(reblock_total as f64);
            }

            metrics::gauge!("lidi_receive_dispatch_queue_len").set(self.for_dispatch.len() as f64);
            metrics::gauge!("lidi_receive_clients_queue_len").set(self.for_clients.len() as f64);

            metrics::gauge!("lidi_receive_active_transfers_len")
                .set(self.active_transfers.len() as f64);

            let (total, max) =
                self.active_transfers
                    .iter()
                    .fold((0usize, 0usize), |(t, m), ref_multi| {
                        let len = ref_multi.value().len();
                        (t + len, m.max(len))
                    });
            metrics::gauge!("lidi_receive_client_sendq_total_len").set(total as f64);
            metrics::gauge!("lidi_receive_client_sendq_max_len").set(max as f64);
        }
    }

    /// Builds a new receiver from the given configuration, `RaptorQ` parameters and client
    /// lifecycle. Workers are not started until [`Receiver::start`] is called.
    ///
    /// # Errors
    ///
    /// Will return `Err` if the receiver cannot be constructed from the configuration.
    pub fn new(
        config: &config::ReceiveConfig,
        raptorq: protocol::RaptorQ,
        client_lifecycle: Lifecycle,
    ) -> Result<Self, Error> {
        let config = Config::from(config);

        let packet_vec_recycler = crossbeam_deque::Injector::new();

        let (to_dispatch, for_dispatch) = match config.dispatch_queue_size {
            0 => crossbeam_channel::unbounded(),
            n => crossbeam_channel::bounded(n),
        };
        let (to_clients, for_clients) = match config.clients_queue_size {
            0 => crossbeam_channel::unbounded(),
            n => crossbeam_channel::bounded(n),
        };

        Ok(Self {
            config,
            raptorq,
            packet_vec_recycler,
            to_dispatch,
            for_dispatch,
            to_clients,
            for_clients,
            active_transfers: dashmap::DashMap::new(),
            failed_transfers: dashmap::DashSet::new(),
            #[cfg(feature = "prometheus")]
            reblock_queues: std::sync::RwLock::new(Vec::new()),
            client_lifecycle,
        })
    }

    /// Spawns all worker threads (per-port recv and reblock workers, the dispatch worker, the
    /// optional metrics worker, and `max_clients` client workers) into `scope`, returning once
    /// they are running.
    ///
    /// # Errors
    ///
    /// Will return `Err` if no UDP port is configured or if a scoped thread cannot be spawned.
    #[allow(clippy::too_many_lines)]
    pub fn start<'a>(&'a self, scope: &'a thread::Scope<'a, '_>) -> Result<(), Error> {
        log::info!(
            "max {} simultaneous clients/transfers",
            self.config.max_clients
        );

        log::info!("receive mode is {}", self.config.mode);
        log::info!("sending to {:?}", self.config.to);

        log::info!(
            "queue sizes: reblock={} dispatch={} clients={} client={}",
            self.config.reblock_queue_size,
            self.config.dispatch_queue_size,
            self.config.clients_queue_size,
            self.config.client_queue_size,
        );

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

        #[cfg(feature = "heartbeat")]
        if let Some(hb_interval) = self.config.heartbeat {
            log::info!(
                "heartbeat interval is set to {} seconds",
                hb_interval.as_secs()
            );
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
                    if let Err(e) = clients::start(self, i) {
                        log::error!("fatal client_{i} error: {e}");
                    }
                })?;
        }

        thread::Builder::new()
            .name(String::from("dispatch"))
            .spawn_scoped(scope, move || {
                if let Err(e) = dispatch::start(self) {
                    log::error!("fatal dispatch error: {e}");
                }
            })?;

        if self.config.ports.is_empty() {
            return Err(Error::Internal(String::from("no ports configured")));
        }

        for port in &self.config.ports {
            let (to_reblock, for_reblock) = match self.config.reblock_queue_size {
                0 => crossbeam_channel::unbounded(),
                n => crossbeam_channel::bounded(n),
            };

            #[cfg(feature = "prometheus")]
            if let Ok(mut queues) = self.reblock_queues.write() {
                queues.push(for_reblock.clone());
            }

            // Recycles the Vec<EncodingPacket> batches built by the udp worker: reblock sends
            // each one back (emptied via drain) once processed, so udp can reuse it instead of
            // allocating, the same pattern as lidi-send's block_recycler. A plain SPSC channel
            // suffices since exactly one reblock thread and one udp thread share it per port.
            #[cfg(feature = "receive-mmsg")]
            thread::Builder::new()
                .name(format!("reblock_{port}"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = reblock::start(self, &for_reblock) {
                        log::error!("fatal reblock error: {e}");
                    }
                })?;

            thread::Builder::new()
                .name(format!("recv_{port}"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = udp::start(self, *port, &to_reblock) {
                        log::error!("fatal recv_{port} error: {e}");
                    }
                })?;
        }

        log::info!(
            "RaptorQ block {} bytes in {} packets + {} repair packets ",
            self.raptorq.block_size(),
            self.raptorq.min_nb_packets(),
            self.raptorq.nb_packets() - self.raptorq.min_nb_packets(),
        );

        log::debug!("{}", self.raptorq);

        Ok(())
    }
}
