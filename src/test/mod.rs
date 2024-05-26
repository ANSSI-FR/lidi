//! Worker that encodes protocol messages into RaptorQ packets

use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use raptorq::ObjectTransmissionInformation;

use crate::protocol::{Message, MessageType};

pub fn build_random_message(oti: ObjectTransmissionInformation) -> Message {
    // set a seed for random algorithm generation
    let mut rng = XorShiftRng::from_seed([
        3, 42, 93, 129, 1, 85, 72, 42, 84, 23, 95, 212, 253, 10, 4, 2,
    ]);

    // get real transfer data size ( remove message header overhead )
    let real_data_size = oti.transfer_length() as usize - Message::serialize_overhead();

    // generate some random data
    let data = (0..real_data_size)
        .map(|_| rng.gen_range(0..=255) as u8)
        .collect::<Vec<_>>();

    // now encode a message
    Message::new(MessageType::Data, data.len() as _, 0, Some(&data))
}

#[cfg(test)]
mod tests {
    use crate::receive::decoding::Decoding;
    use crate::send::encoding::Encoding;

    use crate::protocol::object_transmission_information;

    #[test]
    fn test_encode() {
        // transmission propreties, set by user
        let mtu = 1500;
        let block_size = 60000;
        let repair_block_size = 6000;

        // create configuration based on user configuration
        let object_transmission_info = object_transmission_information(mtu, block_size);

        let message = super::build_random_message(object_transmission_info);

        let original_data = message.serialized().to_owned();

        // create our encoding module
        let encoding = Encoding::new(object_transmission_info, repair_block_size);

        let block_id = 0;
        let packets = encoding.encode(message, block_id);

        // now decode
        let decoder = Decoding::new(object_transmission_info);

        let decoded_data = decoder.decode(packets, block_id).unwrap();

        assert_eq!(original_data, decoded_data);
    }
}
