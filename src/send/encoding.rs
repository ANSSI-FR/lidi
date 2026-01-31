//! Worker that encodes protocol blocks into `RaptorQ` packets

use crate::send;

pub fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    loop {
        let Some((id, block)) = sender.for_encoding.recv()? else {
            sender.to_send.send(None)?;
            return Ok(());
        };

        let client_id = block.client_id();

        let packets = sender.raptorq.encode(id, block.serialized());

        log::debug!("encoding block {id} for client {client_id:x}");

        let mut block_to_send = sender.block_to_send.0.lock().map_err(|e| {
            send::Error::Other(format!("failed to acquire block_to_send mutex: {e}"))
        })?;

        block_to_send = sender
            .block_to_send
            .1
            .wait_while(block_to_send, |block_to_send| *block_to_send != id)
            .map_err(|e| {
                send::Error::Other(format!("failed to wait_while block_to_send mutex: {e}"))
            })?;

        sender.to_send.send(Some(packets))?;

        *block_to_send = block_to_send.wrapping_add(1);
        drop(block_to_send);
        sender.block_to_send.1.notify_all();
    }
}
