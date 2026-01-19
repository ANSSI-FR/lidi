//! Worker that decodes `RaptorQ` packets into protocol blocks

use crate::{protocol, receive};
use std::{sync, thread};

pub fn start<ClientNew, ClientEnd>(
    receiver: &receive::Receiver<ClientNew, ClientEnd>,
) -> Result<(), receive::Error> {
    loop {
        match receiver.for_decode.recv()? {
            super::Reassembled::Block { id, packets } => {
                match receiver.raptorq.decode(id, packets) {
                    None => {
                        log::error!("lost block {id} (failed to decode)");
                        receiver.to_dispatch.send(None)?;
                    }
                    Some(block) => {
                        log::debug!("block {id} decoded with {} bytes!", block.len());

                        'inner: loop {
                            let block_to_dispatch = receiver
                                .block_to_dispatch
                                .load(sync::atomic::Ordering::SeqCst);

                            if block_to_dispatch == id {
                                receiver
                                    .to_dispatch
                                    .send(Some(protocol::Block::deserialize(block)))?;
                                receiver
                                    .block_to_dispatch
                                    .fetch_add(1, sync::atomic::Ordering::SeqCst);
                                break 'inner;
                            }

                            thread::yield_now();
                        }
                    }
                }
            }
            super::Reassembled::Error => {
                log::warn!("synchronization lost received, propagating");
                receiver.to_dispatch.send(None)?;
            }
        }

        thread::yield_now();
    }
}
