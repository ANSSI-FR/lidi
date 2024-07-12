//! Worker that decodes RaptorQ packets into protocol messages

use crate::{protocol, receive};

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
    let encoding_block_size = receiver.object_transmission_info.transfer_length();

    loop {
        let (block_id, packets) = receiver.for_decoding.recv()?;

        let packets = match packets {
            None => {
                log::warn!("synchronization lost received, propagating");
                // Sending lost synchronization signal to reorder thread
                receiver.to_reordering.send((block_id, None))?;
                continue;
            }
            Some(packets) => packets,
        };

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
                // Sending lost synchronization signal to reorder thread
                receiver.to_reordering.send((block_id, None))?;
            }
            Some(block) => {
                log::trace!("block {block_id} decoded with {} bytes!", block.len());
                receiver
                    .to_reordering
                    .send((block_id, Some(protocol::Message::deserialize(block))))?;
            }
        }
    }
}
