use crate::receive::tcp_serve;
use crate::protocol;
use crossbeam_channel::{unbounded, SendError, Sender};
use log::{debug, error, trace};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use std::{fmt, io, net, os::unix::net::UnixStream, thread};

pub struct Config {
    pub logical_block_size: u64,
    pub to_tcp: net::SocketAddr,
    pub to_tcp_buffer_size: usize,
    pub abort_timeout: Duration,
}

enum Error {
    Io(io::Error),
    Crossbeam(SendError<protocol::Message>),
    Diode(protocol::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Crossbeam(e) => write!(fmt, "crossbeam send error: {e}"),
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
        Self::Crossbeam(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Diode(e)
    }
}

pub fn new(config: Config, decoding_recvr: UnixStream) {
    if let Err(e) = main_loop(config, decoding_recvr) {
        error!("deserialize loop error: {e}");
    }
}

fn main_loop(config: Config, decoding_recvr: UnixStream) -> Result<(), Error> {
    let mut decoding_recvr =
        io::BufReader::with_capacity(config.logical_block_size as usize, decoding_recvr);

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

    loop {
        let message: protocol::ClientMessage =
            protocol::ClientMessage::deserialize_from(&mut decoding_recvr)?;

        trace!("received {}", message);

        if failed_transfers.contains(&message.client_id) {
            continue;
        }

        let will_end = matches!(
            message.payload,
            protocol::Message::Abort | protocol::Message::End
        );

        match message.payload {
            protocol::Message::Padding(_) => {
                // use padding messages to expunge ended transfers
                ended_transfers.retain(|client_id, client_sendq| {
                    let retain = client_sendq.is_empty();
                    if !retain {
                        debug!("purging ended transfer of client {client_id:x}");
                    }
                    retain
                });
                continue;
            }

            protocol::Message::Start => {
                let (client_sendq, client_recvq) = unbounded::<protocol::Message>();

                active_transfers.insert(message.client_id, client_sendq);

                let tcp_serve_config = tcp_serve_config.clone();

                thread::spawn(move || {
                    tcp_serve::new(tcp_serve_config, message.client_id, client_recvq)
                });

                continue;
            }

            _ => {
                let client_sendq = active_transfers.get(&message.client_id).unwrap();

                if let Err(e) = client_sendq.send(message.payload) {
                    error!(
                        "failed to send payload to client {:x}: {e}",
                        message.client_id
                    );
                    active_transfers.remove(&message.client_id);
                    failed_transfers.insert(message.client_id);
                    continue;
                }
            }
        }

        if will_end {
            let client_sendq = active_transfers.remove(&message.client_id).unwrap();
            ended_transfers.insert(message.client_id, client_sendq);
        }
    }
}
