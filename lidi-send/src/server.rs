//! Worker that gets a client socket and becomes a `crate::send::client` worker

use crate::client;
use lidi_protocol as protocol;
use std::{io::Read, os::fd::AsRawFd, sync};

static CLIENT_ID_COUNTER: sync::atomic::AtomicU16 = sync::atomic::AtomicU16::new(0);

fn new_client_id() -> protocol::ClientId {
    CLIENT_ID_COUNTER.fetch_add(1, sync::atomic::Ordering::Relaxed)
}

pub fn start<C>(sender: &crate::Sender<C>) -> Result<(), crate::Error>
where
    C: Read + AsRawFd + Send,
{
    loop {
        let Some((endpoint_id, endpoint_options, client)) = sender.for_server.recv()? else {
            for _ in 0..sender.config.ports.len() {
                sender.to_encode.send(None)?;
            }
            return Ok(());
        };

        let client_id = new_client_id();

        let client_res = client::start(sender, endpoint_id, endpoint_options, client_id, client);

        if let Err((sequence_number, e)) = client_res {
            log::error!("client {client_id:x}: error: {e}");

            if let Err(e) = sender.to_encode.send(Some(protocol::Block::new(
                sender.block_recycler.steal().success(),
                protocol::BlockType::Abort,
                &sender.raptorq,
                client_id,
                sequence_number,
                None,
            )?)) {
                log::error!("client {client_id:x}: failed to abort : {e}");
            }
        }
    }
}
