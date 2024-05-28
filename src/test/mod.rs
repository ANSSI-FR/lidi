//! Worker that encodes protocol messages into RaptorQ packets

use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use crate::protocol::{Header, MessageType};

pub fn build_random_data(data_len: usize) -> Vec<u8> {
    // set a seed for random algorithm generation
    let mut rng = XorShiftRng::from_seed([
        3, 42, 93, 129, 1, 85, 72, 42, 84, 23, 95, 212, 253, 10, 4, 2,
    ]);

    // generate some random data
    (0..data_len)
        .map(|_| rng.gen_range(0..=255) as u8)
        .collect::<Vec<_>>()
}

pub fn build_random_message(data_len: usize) -> (Header, Vec<u8>) {
    let header = Header::new(MessageType::Data, 0, 0);
    let data = build_random_data(data_len);

    (header, data)
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

        let real_data_size = object_transmission_info.transfer_length() as usize;
        let (_header, payload) = super::build_random_message(real_data_size);

        let original_data = payload.clone();

        // create our encoding module
        let encoding = Encoding::new(object_transmission_info, repair_block_size);

        let block_id = 0;
        let packets = encoding.encode(payload, block_id);

        // now decode
        let decoder = Decoding::new(object_transmission_info);

        let decoded_data = decoder.decode(packets, block_id).unwrap();

        assert_eq!(original_data, decoded_data);
    }
}
