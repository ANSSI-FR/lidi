use crate::{protocol, receive::tcp_serve, semaphore};
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, SendError, Sender};
use log::{debug, error, trace, warn};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
use std::{fmt, io, net, thread};

pub struct Config {
    pub nb_multiplex: u16,
    pub to_tcp: net::SocketAddr,
    pub to_tcp_buffer_size: usize,
    pub abort_timeout: Duration,
    pub heartbeat: Duration,
}

enum Error {
    Io(io::Error),
    CrossbeamSend(SendError<protocol::Message>),
    CrossbeamRecv(RecvTimeoutError),
    Diode(protocol::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::CrossbeamSend(e) => write!(fmt, "crossbeam send error: {e}"),
            Self::CrossbeamRecv(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::Diode(e) => write!(fmt, "diode error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<SendError<protocol::Message>> for Error {
    fn from(e: SendError<protocol::Message>) -> Self {
        Self::CrossbeamSend(e)
    }
}

impl From<RecvTimeoutError> for Error {
    fn from(e: RecvTimeoutError) -> Self {
        Self::CrossbeamRecv(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Diode(e)
    }
}

pub fn new(config: Config, decoding_recvq: Receiver<protocol::Message>) {
    if let Err(e) = main_loop(config, decoding_recvq) {
        error!("deserialize loop error: {e}");
    }
}

fn main_loop(config: Config, decoding_recvq: Receiver<protocol::Message>) -> Result<(), Error> {
    let mut active_transfers: BTreeMap<protocol::ClientId, Sender<protocol::Message>> =
        BTreeMap::new();
    let mut ended_transfers: BTreeMap<protocol::ClientId, Sender<protocol::Message>> =
        BTreeMap::new();
    let mut failed_transfers: BTreeSet<protocol::ClientId> = BTreeSet::new();

    let tcp_serve_config = tcp_serve::Config {
        to_tcp: config.to_tcp,
        to_tcp_buffer_size: config.to_tcp_buffer_size,
        abort_timeout: config.abort_timeout,
    };

    let multiplex_control = semaphore::Semaphore::new(config.nb_multiplex as usize);

    let mut last_heartbeat = Instant::now();

    loop {
        let message = match decoding_recvq.recv_timeout(config.heartbeat) {
            Err(RecvTimeoutError::Timeout) => {
                if last_heartbeat.elapsed() > config.heartbeat {
                    warn!(
                        "no heartbeat message received during the last {} second(s)",
                        config.heartbeat.as_secs()
                    );
                }
                continue;
            }
            other => other?,
        };

        trace!("received {message}");

        let client_id = message.client_id();

        if failed_transfers.contains(&client_id) {
            continue;
        }

        let message_type = message.message_type()?;

        let mut will_end = false;

        match message_type {
            protocol::MessageType::Heartbeat => {
                last_heartbeat = Instant::now();
                continue;
            }

            protocol::MessageType::Start => {
                let (client_sendq, client_recvq) = unbounded::<protocol::Message>();

                active_transfers.insert(client_id, client_sendq);

                let tcp_serve_config = tcp_serve_config.clone();
                let multiplex_control = multiplex_control.clone();

                thread::Builder::new()
                    .name(format!("client {client_id:x}"))
                    .spawn(move || {
                        tcp_serve::new(tcp_serve_config, multiplex_control, client_id, client_recvq)
                    })
                    .expect("thread spawn");
            }

            protocol::MessageType::Abort | protocol::MessageType::End => will_end = true,

            protocol::MessageType::Data => (),
        }

        trace!("message = {message}");

        let client_sendq = active_transfers.get(&client_id).expect("active transfer");

        if let Err(e) = client_sendq.send(message) {
            error!("failed to send payload to client {client_id:x}: {e}");
            active_transfers.remove(&client_id);
            failed_transfers.insert(client_id);
            continue;
        }

        if will_end {
            let client_sendq = active_transfers
                .remove(&client_id)
                .expect("active transfer");

            ended_transfers.retain(|client_id, client_sendq| {
                let retain = !client_sendq.is_empty();
                if !retain {
                    debug!("purging ended transfer of client {client_id:x}");
                }
                retain
            });

            ended_transfers.insert(client_id, client_sendq);
        }
    }
}
