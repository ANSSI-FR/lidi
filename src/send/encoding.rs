//! Worker that encodes protocol messages into RaptorQ packets

use raptorq::EncodingPacket;

use crate::protocol;

#[derive(Clone)]
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

    pub fn encode(&self, data: Vec<u8>, block_id: u8) -> Vec<EncodingPacket> {
        log::trace!("encoding a serialized block of {} bytes", data.len());

        let encoder = raptorq::SourceBlockEncoder::with_encoding_plan(
            block_id,
            &self.object_transmission_info,
            &data,
            &self.sbep,
        );

        let mut packets = encoder.source_packets();

        if 0 < self.nb_repair_packets {
            packets.extend(encoder.repair_packets(0, self.nb_repair_packets));
        }

        packets
    }
}
