use crate::{protocol, sock_utils};
use std::{
    io::{self, Write},
    os::fd::AsRawFd,
};

pub(crate) fn start<C, F, E>(
    receiver: &super::Receiver<F>,
    client_id: protocol::ClientId,
    recvq: crossbeam_channel::Receiver<protocol::Message>,
) -> Result<(), super::Error>
where
    C: Write + AsRawFd,
    F: Send + Sync + Fn() -> Result<C, E>,
    E: Into<super::Error>,
{
    log::info!("client {client_id:x}: starting transfer");

    let client = (receiver.new_client)().map_err(|e| e.into())?;

    let sock_buffer_size = sock_utils::get_socket_send_buffer_size(&client)?;
    if (sock_buffer_size as usize) < 2 * receiver.to_buffer_size {
        sock_utils::set_socket_send_buffer_size(&client, receiver.to_buffer_size as i32)?;
        let new_sock_buffer_size = sock_utils::get_socket_send_buffer_size(&client)?;
        log::info!(
            "client socket send buffer size set to {}",
            new_sock_buffer_size
        );
        if (new_sock_buffer_size as usize) < 2 * receiver.to_buffer_size {
            log::warn!(
                "client socket send buffer may be too small to achieve optimal performances"
            );
            log::warn!("Please review the kernel parameters using sysctl");
        }
    }

    let mut client = io::BufWriter::with_capacity(receiver.to_buffer_size, client);

    let mut transmitted = 0;

    loop {
        match recvq.recv_timeout(receiver.config.abort_timeout) {
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                log::warn!("client {client_id:x}: transfer timeout, aborting");
                return Err(super::Error::from(
                    crossbeam_channel::RecvTimeoutError::Timeout,
                ));
            }
            Err(e) => return Err(super::Error::from(e)),
            Ok(message) => {
                let message_type = message.message_type()?;

                let payload = message.payload();

                if !payload.is_empty() {
                    log::trace!("client {client_id:x}: payload {} bytes", payload.len());
                    transmitted += payload.len();
                    client.write_all(payload)?;
                }

                match message_type {
                    protocol::MessageType::Abort => {
                        log::warn!("client {client_id:x}: aborting transfer");
                        return Ok(());
                    }
                    protocol::MessageType::End => {
                        log::info!("client {client_id:x}: finished transfer, {transmitted} bytes transmitted");
                        client.flush()?;
                        return Ok(());
                    }
                    _ => (),
                }
            }
        }
    }
}
