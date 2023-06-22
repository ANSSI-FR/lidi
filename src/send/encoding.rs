//! Worker that encodes protocol messages into RaptorQ packets

use crate::{protocol, send};

pub(crate) fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    let nb_repair_packets = protocol::nb_repair_packets(
        &sender.object_transmission_info,
        sender.config.repair_block_size,
    );

    if nb_repair_packets == 0 {
        log::warn!("configuration produces 0 repair packet");
    }

    let sbep = raptorq::SourceBlockEncodingPlan::generate(
        (sender.object_transmission_info.transfer_length()
            / u64::from(sender.object_transmission_info.symbol_size())) as u16,
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

        let data = message.serialized();

        log::trace!("encoding a serialized block of {} bytes", data.len());

        let encoder = raptorq::SourceBlockEncoder::with_encoding_plan2(
            block_id,
            &sender.object_transmission_info,
            data,
            &sbep,
        );

        let mut packets = encoder.source_packets();

        if 0 < nb_repair_packets {
            packets.extend(encoder.repair_packets(0, nb_repair_packets));
        }

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
