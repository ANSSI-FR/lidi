//! Worker that gets a client socket and becomes a `crate::send::client` worker

use crate::{protocol, send, send::client};
use std::{io::Read, os::fd::AsRawFd, sync};

pub fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error>
where
    C: Read + AsRawFd + Send,
{
    loop {
        let Some(client) = sender.for_server.recv()? else {
            for _ in 0..sender.config.to_ports.len() {
                sender.to_udp.send(None)?;
            }
            return Ok(());
        };

        let client_id = protocol::new_client_id();

        let client_res = client::start(sender, client_id, client);

        if let Err(e) = client_res {
            log::error!("client {client_id:x}: error: {e}");

            let block_id = sender
                .next_block
                .fetch_add(1, sync::atomic::Ordering::SeqCst);

            if let Err(e) = sender.to_udp.send(Some((
                block_id,
                protocol::Block::new(protocol::BlockType::Abort, &sender.raptorq, client_id, None)?,
            ))) {
                log::error!("client {client_id:x}: failed to abort : {e}");
            }
        }
    }
}
