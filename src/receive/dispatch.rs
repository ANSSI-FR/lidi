use crate::{protocol, receive::tcp_serve, semaphore};
use crossbeam_channel::{unbounded, Receiver, RecvError, SendError, Sender};
use log::{error, trace};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use std::{fmt, io, net, thread};

pub struct Config {
    pub nb_multiplex: u16,
    pub logical_block_size: u64,
    pub to_tcp: net::SocketAddr,
    pub to_tcp_buffer_size: usize,
    pub abort_timeout: Duration,
}

enum Error {
    Io(io::Error),
    CrossbeamSend(SendError<protocol::ClientMessage>),
    CrossbeamRecv(RecvError),
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

impl From<SendError<protocol::ClientMessage>> for Error {
    fn from(e: SendError<protocol::ClientMessage>) -> Self {
        Self::CrossbeamSend(e)
    }
}

impl From<RecvError> for Error {
    fn from(e: RecvError) -> Self {
        Self::CrossbeamRecv(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Diode(e)
    }
}

pub type Message = protocol::ClientMessage;

pub fn new(config: Config, decoding_recvq: Receiver<Message>) {
    if let Err(e) = main_loop(config, decoding_recvq) {
        error!("deserialize loop error: {e}");
    }
}

fn main_loop(config: Config, decoding_recvq: Receiver<Message>) -> Result<(), Error> {
    let mut active_transfers: BTreeMap<protocol::ClientId, Sender<tcp_serve::Message>> =
        BTreeMap::new();
    let mut ended_transfers: BTreeMap<protocol::ClientId, Sender<tcp_serve::Message>> =
        BTreeMap::new();
    let mut failed_transfers: BTreeSet<protocol::ClientId> = BTreeSet::new();

    let tcp_serve_config = tcp_serve::Config {
        to_tcp: config.to_tcp,
        to_tcp_buffer_size: config.to_tcp_buffer_size,
        abort_timeout: config.abort_timeout,
    };

    let multiplex_control = semaphore::Semaphore::new(config.nb_multiplex as usize);

    loop {
        let message = decoding_recvq.recv()?;

        trace!("received {}", message);

        let client_id = message.client_id();

        if failed_transfers.contains(&client_id) {
            continue;
        }

        let message_type = message.message_type()?;

        let mut will_end = false;

        match message_type {
            protocol::MessageType::Start => {
                let (client_sendq, client_recvq) = unbounded::<tcp_serve::Message>();

                active_transfers.insert(client_id, client_sendq);

                let tcp_serve_config = tcp_serve_config.clone();
                let multiplex_control = multiplex_control.clone();

                thread::Builder::new()
                    .name(format!("client {}", client_id))
                    .spawn(move || {
                        tcp_serve::new(tcp_serve_config, multiplex_control, client_id, client_recvq)
                    })
                    .unwrap();
            }
            protocol::MessageType::Abort | protocol::MessageType::End => will_end = true,
            _ => (),
        }

        let client_sendq = active_transfers.get(&client_id).unwrap();

        if let Err(e) = client_sendq.send(message) {
            error!("failed to send payload to client {:x}: {e}", client_id);
            active_transfers.remove(&client_id);
            failed_transfers.insert(client_id);
            continue;
        }

        if will_end {
            let client_sendq = active_transfers.remove(&client_id).unwrap();
            ended_transfers.insert(client_id, client_sendq);
        }
    }
}
