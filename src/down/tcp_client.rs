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
    Crossbeam(SendError<diode::ClientMessage>),
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

impl From<SendError<diode::ClientMessage>> for Error {
    fn from(e: SendError<diode::ClientMessage>) -> Self {
        Self::Crossbeam(e)
    }
}

pub(crate) fn new(config: &Config, client: TcpStream, sendq: Sender<diode::ClientMessage>) {
    let client_id = diode::new_client_id();

    if let Err(e) = main_loop(config, client_id, client, &sendq) {
        error!("client {client_id:x}: error: {e}");

        if let Err(e) = sendq.send(diode::ClientMessage {
            client_id,
            payload: diode::Message::Abort,
        }) {
            error!("client {client_id:x}: failed to abort : {e}");
        }
    }
}

fn main_loop(
    config: &Config,
    client_id: diode::ClientId,
    mut client: TcpStream,
    sendq: &Sender<diode::ClientMessage>,
) -> Result<(), Error> {

    info!("client {client_id:x}: connected");

    let mut buffer = vec![0; config.buffer_size];
    let mut cursor = 0;
    let mut transmitted = 0;

    // close useless upstream
    client.shutdown(std::net::Shutdown::Write)?;

    sendq.send(diode::ClientMessage {
        client_id,
        payload: diode::Message::Start,
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

                    sendq.send(diode::ClientMessage {
                        client_id,
                        payload: diode::Message::Data(buffer[..cursor].into()),
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

                sendq.send(diode::ClientMessage {
                    client_id,
                    payload: diode::Message::Data(buffer.clone()),
                })?;

                cursor = 0;
            }
        }
    }

    sendq.send(diode::ClientMessage {
        client_id,
        payload: diode::Message::End,
    })?;

    info!("client {client_id:x}: disconnect, {transmitted} bytes transmitted");

    Ok(())
}
