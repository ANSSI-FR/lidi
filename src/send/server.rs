//! Worker that gets a client socket and becomes a `crate::send::client` worker

use crate::{protocol, send, send::client};
use std::{io::Read, os::fd::AsRawFd};

pub(crate) fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error>
where
    C: Read + AsRawFd + Send,
{
    loop {
        let client = sender.for_server.recv()?;

        log::debug!("try to acquire multiplex access..");
        sender.multiplex_control.acquire();
        log::debug!("multiplex access acquired");

        let client_id = protocol::new_client_id();

        let client_res = client::start(sender, client_id, client);

        sender.multiplex_control.release();

        if let Err(e) = client_res {
            log::error!("client {client_id:x}: error: {e}");

            if let Err(e) = sender.to_encoding.send(protocol::Message::new(
                protocol::MessageType::Abort,
                sender.from_buffer_size,
                client_id,
                None,
            )) {
                log::error!("client {client_id:x}: failed to abort : {e}");
            }
        }
    }
}
