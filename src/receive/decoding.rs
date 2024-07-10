//! Worker that decodes RaptorQ packets into protocol messages

use std::{cmp::Ordering, thread::yield_now};

use log::warn;

use crate::{protocol, receive};

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
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
                log::warn!("lost block {block_id}");
                continue;
            }
            Some(block) => {
                log::trace!("block {} decoded with {} bytes!", block_id, block.len());

                loop {
                    let mut to_receive = receiver.block_to_receive.lock().expect("acquire lock");
                    match block_id.cmp(&to_receive) {
                        Ordering::Equal => {
                            receiver
                                .to_dispatch
                                .send(protocol::Message::deserialize(block))?;
                            *to_receive = to_receive.wrapping_add(1);
                            break;
                        }
                        Ordering::Less => {
                            // Thread is too late, drop the packet and kill the current job
                            warn!("Dropping the packet {block_id}");
                            break;
                        }
                        Ordering::Greater => {
                            yield_now();
                        }
                    }
                }
            }
        }
    }
}
