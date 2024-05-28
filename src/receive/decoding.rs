//! Worker that decodes RaptorQ packets into protocol messages

use raptorq::ObjectTransmissionInformation;

use raptorq::EncodingPacket;

pub struct Decoding {
    object_transmission_info: raptorq::ObjectTransmissionInformation,
}

impl Decoding {
    pub fn new(object_transmission_info: ObjectTransmissionInformation) -> Decoding {
        Self {
            object_transmission_info,
        }
    }

    pub fn decode(&self, packets: Vec<EncodingPacket>, block_id: u8) -> Option<Vec<u8>> {
        let encoding_block_size = self.object_transmission_info.transfer_length();

        let mut decoder = raptorq::SourceBlockDecoder::new(
            block_id,
            &self.object_transmission_info,
            encoding_block_size,
        );

        decoder.decode(packets)
    }
}
