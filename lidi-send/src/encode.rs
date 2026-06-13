//! Worker that encodes protocol blocks into `RaptorQ` packets

pub fn start<C>(
    sender: &crate::Sender<C>,
    to_udp: &crossbeam_channel::Sender<Option<Vec<raptorq::EncodingPacket>>>,
) -> Result<(), crate::Error> {
    let mut block_id = 0;

    loop {
        let Some(block) = sender.for_encode.recv()? else {
            to_udp.send(None)?;
            return Ok(());
        };

        let client_id = block.client_id();

        log::trace!("encoding block {block_id} for client {client_id:x}");

        let packets = sender.raptorq.encode(block_id, block.serialized());

        sender.block_recycler.push(block);

        to_udp.send(Some(packets))?;

        block_id = block_id.wrapping_add(1);
    }
}
