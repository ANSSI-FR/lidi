//! Worker that reads data from a client socket and split it into [crate::protocol] messages

use crate::{protocol, send, sock_utils};
use std::{io, os::fd::AsRawFd};

pub(crate) fn start<C>(
    sender: &send::Sender<C>,
    client_id: protocol::ClientId,
    mut client: C,
) -> Result<(), send::Error>
where
    C: io::Read + AsRawFd + Send,
{
    log::info!("client {client_id:x}: connected");

    let mut buffer = vec![0; sender.from_buffer_size as usize];
    let mut cursor = 0;
    let mut transmitted = 0;

    let sock_buffer_size = sock_utils::get_socket_recv_buffer_size(&client)?;
    if (sock_buffer_size as u32) < 2 * sender.from_buffer_size {
        sock_utils::set_socket_recv_buffer_size(&client, sender.from_buffer_size as i32)?;
        let new_sock_buffer_size = sock_utils::get_socket_recv_buffer_size(&client)?;
        log::debug!(
            "client socket recv buffer size set to {}",
            new_sock_buffer_size
        );
        if (new_sock_buffer_size as u32) < 2 * sender.from_buffer_size {
            log::warn!(
                "client socket recv buffer may be too small to achieve optimal performances"
            );
        }
    }

    let mut is_first = true;

    loop {
        log::trace!("client {client_id:x}: read...");

        match client.read(&mut buffer[cursor..]) {
            Err(e) => match e.kind() {
                io::ErrorKind::WouldBlock => {
                    if 0 < cursor {
                        log::debug!("client {client_id:x}: flushing pending data");

                        transmitted += cursor;

                        let message_type = if is_first {
                            protocol::MessageType::Start
                        } else {
                            protocol::MessageType::Data
                        };

                        is_first = false;

                        sender.to_encoding.send(protocol::Message::new(
                            message_type,
                            sender.from_buffer_size,
                            client_id,
                            Some(&buffer[..cursor]),
                        ))?;

                        cursor = 0;
                    }
                }
                _ => return Err(e.into()),
            },
            Ok(0) => {
                log::trace!("client {client_id:x}: end of stream");

                if 0 < cursor {
                    // handling incomplete last packet
                    log::trace!("client {client_id:x}: send last buffer");

                    transmitted += cursor;

                    let message_type = if is_first {
                        protocol::MessageType::Start
                    } else {
                        protocol::MessageType::Data
                    };

                    is_first = false;

                    sender.to_encoding.send(protocol::Message::new(
                        message_type,
                        sender.from_buffer_size,
                        client_id,
                        Some(&buffer[..cursor]),
                    ))?;
                }

                if !is_first {
                    sender.to_encoding.send(protocol::Message::new(
                        protocol::MessageType::End,
                        sender.from_buffer_size,
                        client_id,
                        None,
                    ))?;
                }

                log::info!("client {client_id:x}: disconnect, {transmitted} bytes transmitted");

                return Ok(());
            }

            Ok(nread) => {
                log::trace!("client {client_id:x}: {nread} bytes read");

                if (cursor + nread) < sender.from_buffer_size as usize {
                    // buffer is not full
                    log::trace!("client {client_id:x}: buffer is not full, looping");
                    cursor += nread;
                    continue;
                }

                // buffer is full
                log::trace!(
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

                sender.to_encoding.send(protocol::Message::new(
                    message_type,
                    sender.from_buffer_size,
                    client_id,
                    Some(&buffer),
                ))?;

                cursor = 0;
            }
        }
    }
}
