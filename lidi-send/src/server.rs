//! Worker that gets a client socket and becomes a `crate::send::client` worker

use crate::client;
use lidi_protocol as protocol;
use std::{io::Read, os::fd::AsRawFd, sync};

static CLIENT_ID_COUNTER: sync::atomic::AtomicU32 = sync::atomic::AtomicU32::new(0);

fn new_client_id() -> protocol::ClientId {
    CLIENT_ID_COUNTER.fetch_add(1, sync::atomic::Ordering::Relaxed)
}

pub fn start<C>(sender: &crate::Sender<C>) -> Result<(), crate::Error>
where
    C: Read + AsRawFd + Send,
{
    loop {
        let Some((endpoint, client)) = sender.for_server.recv()? else {
            for _ in 0..sender.config.ports.len() {
                sender.to_udp.send(None)?;
            }
            return Ok(());
        };

        let client_id = new_client_id();

        let client_res = client::start(sender, endpoint, client_id, client);

        if let Err(e) = client_res {
            log::error!("client {client_id:x}: error: {e}");

            let block_id = sender
                .next_block
                .fetch_add(1, sync::atomic::Ordering::SeqCst);

            if let Err(e) = sender.to_udp.send(Some(protocol::Block::new(
                sender.block_recycler.steal().success(),
                block_id,
                protocol::BlockType::Abort,
                &sender.raptorq,
                client_id,
                None,
            )?)) {
                log::error!("client {client_id:x}: failed to abort : {e}");
            }
        }
    }
}
