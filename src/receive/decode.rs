//! Worker that decodes `RaptorQ` packets into protocol blocks

use crate::{protocol, receive};
use std::thread;

pub fn start<ClientNew, ClientEnd>(
    receiver: &receive::Receiver<ClientNew, ClientEnd>,
) -> Result<(), receive::Error> {
    loop {
        match receiver.for_decode.recv()? {
            super::Reassembled::Block { id, packets } => {
                log::debug!("received block {id} to decode");

                match receiver.raptorq.decode(id, packets) {
                    None => {
                        log::error!("lost block {id} (failed to decode)");
                        receiver.to_dispatch.send(None)?;
                    }
                    Some(block) => {
                        log::debug!("block {id} decoded with {} bytes!", block.len());

                        let mut block_to_dispatch =
                            receiver.block_to_dispatch.0.lock().map_err(|e| {
                                receive::Error::Other(format!(
                                    "failed to acquire block_to_dispatch mutex: {e}"
                                ))
                            })?;

                        block_to_dispatch = receiver
                            .block_to_dispatch
                            .1
                            .wait_while(block_to_dispatch, |block_to_dispatch| {
                                *block_to_dispatch != id
                            })
                            .map_err(|e| {
                                receive::Error::Other(format!(
                                    "failed to wait_while block_to_dispatch mutex: {e}"
                                ))
                            })?;

                        receiver
                            .to_dispatch
                            .send(Some(protocol::Block::deserialize(block)))?;

                        *block_to_dispatch = block_to_dispatch.wrapping_add(1);
                        drop(block_to_dispatch);
                        receiver.block_to_dispatch.1.notify_all();
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
