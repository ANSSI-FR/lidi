//! Worker that decodes `RaptorQ` packets into protocol blocks

use crate::{protocol, receive};
use std::thread;

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
    loop {
        if receiver.broken_pipeline.load() {
            return Ok(());
        }

        match receiver.for_decode.recv()? {
            super::Reassembled::Error => {
                log::warn!("synchronization lost received, propagating");
                receiver.to_dispatch.send(None)?;
                continue;
            }
            super::Reassembled::Block { id, packets } => {
                match receiver.raptorq.decode(id, packets) {
                    None => {
                        log::error!("lost block {id} (failed to decode)");
                        receiver.to_dispatch.send(None)?;
                    }
                    Some(block) => {
                        log::debug!("block {id} decoded with {} bytes!", block.len());
                        receiver
                            .to_dispatch
                            .send(Some(protocol::Block::deserialize(block)))?;
                    }
                }
            }
        }

        thread::yield_now();
    }
}
