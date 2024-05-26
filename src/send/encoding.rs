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
