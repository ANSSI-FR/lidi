//! Worker that decodes RaptorQ packets into protocol messages

use crate::{protocol, receive};

pub(crate) fn start<F>(
    receiver: &receive::Receiver<F>,
    nb_decoding_threads: u8,
) -> Result<(), receive::Error> {
    let encoding_block_size = receiver.object_transmission_info.transfer_length();

    loop {
        let (block_id, packets) = receiver.for_decoding.recv()?;

        log::trace!(
            "trying to decode block {block_id} with {} packets",
            packets.len()
        );

        let mut decoder = raptorq::SourceBlockDecoder::new(
            block_id,
            &receiver.object_transmission_info,
            encoding_block_size,
        );

        match decoder.decode(packets) {
            None => {
                log::error!("lost block {block_id}, synchronization lost");
                // Sending lost synchronization signal to dispatch
                receiver.to_dispatch.send(None)?;
                continue;
            }
            Some(block) => {
                log::trace!("block {block_id} decoded with {} bytes!", block.len());

                let mut retried = 0;

                loop {
                    let mut to_receive = receiver.block_to_receive.lock().expect("acquire lock");

                    if *to_receive == block_id {
                        // The decoded block is the expected one, dispatching it
                        receiver
                            .to_dispatch
                            .send(Some(protocol::Message::deserialize(block)))?;
                        *to_receive = to_receive.wrapping_add(1);
                        break;
                    } else {
                        // The decoded block is not the expected one
                        // Retrying until all decoding threads had one chance to dispatch their block
                        if nb_decoding_threads < retried {
                            // All decoding threads should have had one chance to dispatch their block
                            log::warn!("dropping block {block_id} after trying to dispatch it {retried} times");

                            // Sending lost synchronization signal to dispatch
                            receiver.to_dispatch.send(None)?;

                            break;
                        }
                        retried += 1;
                    }
                }
            }
        }
    }
}
