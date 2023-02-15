use crate::{protocol, semaphore};
use crossbeam_channel::{SendError, Sender};
use log::{debug, error, info, trace};
use std::{
    fmt,
    io::{self, Read},
    net::TcpStream,
};

#[derive(Clone)]
pub struct Config {
    pub buffer_size: u32,
}

enum Error {
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

pub fn new(
    config: &Config,
    multiplex_control: &semaphore::Semaphore,
    sendq: Sender<protocol::ClientMessage>,
    client: TcpStream,
) {
    debug!("try to acquire multiplex access..");

    multiplex_control.acquire();

    debug!("multiplex access acquired");

    let client_id = protocol::new_client_id();

    if let Err(e) = main_loop(config, client_id, client, &sendq) {
        error!("client {client_id:x}: error: {e}");

        if let Err(e) = sendq.send(protocol::ClientMessage::new(
            protocol::MessageType::Abort,
            config.buffer_size,
            client_id,
            None,
        )) {
            error!("client {client_id:x}: failed to abort : {e}");
        }
    }

    multiplex_control.release()
}

fn main_loop(
    config: &Config,
    client_id: protocol::ClientId,
    mut client: TcpStream,
    sendq: &Sender<protocol::ClientMessage>,
) -> Result<(), Error> {
    info!("client {client_id:x}: connected");

    let mut buffer = vec![0; config.buffer_size as usize];
    let mut cursor = 0;
    let mut transmitted = 0;

    // close useless upstream
    client.shutdown(std::net::Shutdown::Write)?;

    let mut is_first = true;

    loop {
        trace!("client {client_id:x}: read...");

        match client.read(&mut buffer[cursor..])? {
            0 => {
                trace!("client {client_id:x}: end of stream");
                if 0 < cursor {
                    // handling incomplete last packet
                    trace!("client {client_id:x}: send last buffer");

                    transmitted += cursor;

                    let message_type = if is_first {
                        protocol::MessageType::Start
                    } else {
                        protocol::MessageType::Data
                    };

                    let message = protocol::ClientMessage::new(
                        message_type,
                        config.buffer_size,
                        client_id,
                        Some(&buffer[..cursor]),
                    );

                    sendq.send(message)?;
                }
                break;
            }
            nread => {
                trace!("client {client_id:x}: {nread} bytes read");
                if (cursor + nread) < config.buffer_size as usize {
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

                let message_type = if is_first {
                    protocol::MessageType::Start
                } else {
                    protocol::MessageType::Data
                };

                let message = protocol::ClientMessage::new(
                    message_type,
                    config.buffer_size,
                    client_id,
                    Some(&buffer),
                );

                is_first = false;

                sendq.send(message)?;

                cursor = 0;
            }
        }
    }

    sendq.send(protocol::ClientMessage::new(
        protocol::MessageType::End,
        config.buffer_size,
        client_id,
        None,
    ))?;

    info!("client {client_id:x}: disconnect, {transmitted} bytes transmitted");

    Ok(())
}
