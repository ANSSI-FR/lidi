use crate::{protocol, semaphore, sock_utils};
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
    Crossbeam(SendError<protocol::Message>),
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

impl From<SendError<protocol::Message>> for Error {
    fn from(e: SendError<protocol::Message>) -> Self {
        Self::Crossbeam(e)
    }
}

pub fn new(
    config: &Config,
    multiplex_control: &semaphore::Semaphore,
    sendq: Sender<protocol::Message>,
    client: TcpStream,
) {
    debug!("try to acquire multiplex access..");

    multiplex_control.acquire();

    debug!("multiplex access acquired");

    let client_id = protocol::new_client_id();

    if let Err(e) = main_loop(config, client_id, client, &sendq) {
        error!("client {client_id:x}: error: {e}");

        if let Err(e) = sendq.send(protocol::Message::new(
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
    sendq: &Sender<protocol::Message>,
) -> Result<(), Error> {
    info!("client {client_id:x}: connected");

    let mut buffer = vec![0; config.buffer_size as usize];
    let mut cursor = 0;
    let mut transmitted = 0;

    client.shutdown(std::net::Shutdown::Write)?;
    let sock_buffer_size = sock_utils::get_socket_recv_buffer_size(&client);
    if (sock_buffer_size as u32) < 2 * config.buffer_size {
        sock_utils::set_socket_recv_buffer_size(&client, config.buffer_size as i32);
        let new_sock_buffer_size = sock_utils::get_socket_recv_buffer_size(&client);
        log::info!(
            "TCP socket recv buffer size set to {}",
            new_sock_buffer_size
        );
        if (new_sock_buffer_size as u32) < 2 * config.buffer_size {
            log::warn!("TCP socket recv buffer may be too small to achieve optimal performances");
        }
    }

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

                    sendq.send(protocol::Message::new(
                        message_type,
                        config.buffer_size,
                        client_id,
                        Some(&buffer[..cursor]),
                    ))?;
                }

                sendq.send(protocol::Message::new(
                    protocol::MessageType::End,
                    config.buffer_size,
                    client_id,
                    None,
                ))?;

                info!("client {client_id:x}: disconnect, {transmitted} bytes transmitted");

                return Ok(());
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

                is_first = false;

                sendq.send(protocol::Message::new(
                    message_type,
                    config.buffer_size,
                    client_id,
                    Some(&buffer),
                ))?;

                cursor = 0;
            }
        }
    }
}
