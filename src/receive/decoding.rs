//! Worker that decodes RaptorQ packets into protocol messages

use raptorq::EncodingPacket;

use crate::{protocol, receive};

pub struct Decoding {
    object_transmission_info: raptorq::ObjectTransmissionInformation,
}

impl Decoding {
    pub fn new(object_transmission_info: raptorq::ObjectTransmissionInformation) -> Decoding {
        Self {
            object_transmission_info,
        }
    }

    pub fn decode(&self, packets: Vec<EncodingPacket>, block_id: u8) -> Option<Vec<u8>> {
        let encoding_block_size = self.object_transmission_info.transfer_length();

        let mut decoder = raptorq::SourceBlockDecoder::new2(
            block_id,
            &self.object_transmission_info,
            encoding_block_size,
        );

        decoder.decode(packets)
    }
}

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
    let decoder = Decoding::new(receiver.object_transmission_info);

    loop {
        let (block_id, packets) = receiver.for_decoding.recv()?;

        log::trace!(
            "trying to decode block {block_id} with {} packets",
            packets.len()
        );

        match decoder.decode(packets, block_id) {
            None => {
                log::warn!("lost block {block_id}");
                continue;
            }
            Some(block) => {
                log::trace!("block {} decoded with {} bytes!", block_id, block.len());

                loop {
                    let mut to_receive = receiver.block_to_receive.lock().expect("acquire lock");
                    if *to_receive == block_id {
                        receiver
                            .to_dispatch
                            .send(protocol::Message::deserialize(block))?;
                        *to_receive = to_receive.wrapping_add(1);
                        break;
                    }
                }
            }
        }
    }
}
