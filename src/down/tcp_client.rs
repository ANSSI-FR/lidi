use crossbeam_channel::{SendError, Sender};
use log::{error, info, trace};
use std::{
    fmt,
    io::{self, Read},
    net::TcpStream,
};

#[derive(Clone)]
pub(crate) struct Config {
    pub buffer_size: usize,
}

pub(crate) enum Error {
    Io(io::Error),
    Crossbeam(SendError<protocol::ClientMessage>),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Crossbeam(e) => write!(fmt, "crossbeam error: {e}"),
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
        Self::Crossbeam(e)
    }
}

pub(crate) fn new(config: &Config, client: TcpStream, sendq: Sender<protocol::ClientMessage>) {
    let client_id = protocol::new_client_id();

    if let Err(e) = main_loop(config, client_id, client, &sendq) {
        error!("client {client_id:x}: error: {e}");

        if let Err(e) = sendq.send(protocol::ClientMessage {
            client_id,
            payload: protocol::Message::Abort,
        }) {
            error!("client {client_id:x}: failed to abort : {e}");
        }
    }
}

fn main_loop(
    config: &Config,
    client_id: protocol::ClientId,
    mut client: TcpStream,
    sendq: &Sender<protocol::ClientMessage>,
) -> Result<(), Error> {
    info!("client {client_id:x}: connected");

    let mut buffer = vec![0; config.buffer_size];
    let mut cursor = 0;
    let mut transmitted = 0;

    // close useless upstream
    client.shutdown(std::net::Shutdown::Write)?;

    sendq.send(protocol::ClientMessage {
        client_id,
        payload: protocol::Message::Start,
    })?;

    loop {
        trace!("client {client_id:x}: read...");
        match client.read(&mut buffer[cursor..])? {
            0 => {
                trace!("client {client_id:x}: end of stream");
                if 0 < cursor {
                    // handling incomplete last packet
                    trace!("client {client_id:x}: send last buffer");

                    transmitted += cursor;

                    sendq.send(protocol::ClientMessage {
                        client_id,
                        payload: protocol::Message::Data(buffer[..cursor].into()),
                    })?;
                }
                break;
            }
            nread => {
                trace!("client {client_id:x}: {nread} bytes read");
                if (cursor + nread) < config.buffer_size {
                    // buffer is not full
                    trace!("client {client_id:x}: buffer is not full, looping");
                    cursor += nread;
                    continue;
                }

                // buffer is full
                trace!(
                    "client {client_id:x}: send full buffer ({} bytes)",
                    buffer.len()
                );

                transmitted += buffer.len();

                sendq.send(protocol::ClientMessage {
                    client_id,
                    payload: protocol::Message::Data(buffer.clone()),
                })?;

                cursor = 0;
            }
        }
    }

    sendq.send(protocol::ClientMessage {
        client_id,
        payload: protocol::Message::End,
    })?;

    info!("client {client_id:x}: disconnect, {transmitted} bytes transmitted");

    Ok(())
}
