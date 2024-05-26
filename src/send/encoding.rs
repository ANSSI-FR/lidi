//! Worker that encodes protocol messages into RaptorQ packets

use raptorq::EncodingPacket;

use crate::{
    protocol::{self, Message},
    send,
};

pub struct Encoding {
    nb_repair_packets: u32,
    object_transmission_info: raptorq::ObjectTransmissionInformation,
    sbep: raptorq::SourceBlockEncodingPlan,
}

impl Encoding {
    pub fn new(
        object_transmission_info: raptorq::ObjectTransmissionInformation,
        repair_block_size: u32,
    ) -> Encoding {
        let nb_repair_packets =
            protocol::nb_repair_packets(&object_transmission_info, repair_block_size);

        if nb_repair_packets == 0 {
            log::warn!("configuration produces 0 repair packet");
        }

        let sbep = raptorq::SourceBlockEncodingPlan::generate(
            (object_transmission_info.transfer_length()
                / u64::from(object_transmission_info.symbol_size())) as u16,
        );

        Self {
            nb_repair_packets,
            object_transmission_info,
            sbep,
        }
    }

    pub fn encode(&self, message: Message, block_id: u8) -> Vec<EncodingPacket> {
        let data = message.serialized();

        log::trace!("encoding a serialized block of {} bytes", data.len());

        let encoder = raptorq::SourceBlockEncoder::with_encoding_plan2(
            block_id,
            &self.object_transmission_info,
            data,
            &self.sbep,
        );

        let mut packets = encoder.source_packets();

        if 0 < self.nb_repair_packets {
            packets.extend(encoder.repair_packets(0, self.nb_repair_packets));
        }

        packets
    }
}

pub(crate) fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    let encoding = Encoding::new(
        sender.object_transmission_info,
        sender.config.repair_block_size,
    );

    loop {
        let mut block_id_to_encode = sender.block_to_encode.lock().expect("acquire lock");
        let message = sender.for_encoding.recv()?;
        let block_id = *block_id_to_encode;
        *block_id_to_encode = block_id_to_encode.wrapping_add(1);
        drop(block_id_to_encode);

        let message_type = message.message_type()?;
        let client_id = message.client_id();

        match message_type {
            protocol::MessageType::Start => log::debug!(
                "start of encoding block {block_id} for client {:x}",
                client_id
            ),
            protocol::MessageType::End => log::debug!(
                "end of encoding block {block_id} for client {:x}",
                client_id
            ),
            _ => (),
        }

        let packets = encoding.encode(message, block_id);

        loop {
            let mut to_send = sender.block_to_send.lock().expect("acquire lock");
            if *to_send == block_id {
                sender.to_send.send(packets)?;
                *to_send = to_send.wrapping_add(1);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{object_transmission_information, Message, MessageType};
    use crate::send::encoding::Encoding;
    use rand::{Rng, SeedableRng};
    use rand_xorshift::XorShiftRng;

    #[test]
    fn test_encode() {
        // set a seed for random algorithm generation
        let mut rng = XorShiftRng::from_seed([
            3, 42, 93, 129, 1, 85, 72, 42, 84, 23, 95, 212, 253, 10, 4, 2,
        ]);

        // transmission propreties, set by user
        let mtu = 1500;
        let block_size = 60000;
        let repair_block_size = 6000;

        // create configuration based on user configuration
        let object_transmission_info = object_transmission_information(mtu, block_size);

        // get real transfer size (algorithm constraint)
        let block_size = object_transmission_info.transfer_length() as usize;

        // create our encoding module
        let encoding = Encoding::new(object_transmission_info, repair_block_size);

        // get real transfer data size ( remove message header overhead )
        let real_data_size = block_size - Message::serialize_overhead();

        // generate some random data
        let data = (0..real_data_size)
            .map(|_| rng.gen_range(0..=255) as u8)
            .collect::<Vec<_>>();

        // now encode a message
        let block_id = 0;
        let message = Message::new(MessageType::Data, data.len() as _, 0, Some(&data));
        encoding.encode(message, block_id);
    }
}
