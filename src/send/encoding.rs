//! Worker that encodes protocol blocks into `RaptorQ` packets

use crate::send;
use std::thread;

pub(crate) fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    loop {
        let mut block_id_to_encode = sender
            .block_to_encode
            .lock()
            .map_err(|e| send::Error::Other(format!("failed to acquire lock: {e}")))?;
        let Some(block) = sender.for_encoding.recv()? else {
            sender.to_send.send(None)?;
            return Ok(());
        };

        let block_id = *block_id_to_encode;
        *block_id_to_encode = block_id_to_encode.wrapping_add(1);

        // explicitly release the mutex
        drop(block_id_to_encode);

        let client_id = block.client_id();

        log::debug!("encoding block {block_id} for client {client_id:x}");

        let packets = sender.raptorq.encode(block_id, block.serialized());

        loop {
            let mut to_send = sender
                .block_to_send
                .lock()
                .map_err(|e| send::Error::Other(format!("failed to acquire lock: {e}")))?;

            if *to_send == block_id {
                log::trace!("send block {block_id}");
                sender.to_send.send(Some(packets))?;
                *to_send = to_send.wrapping_add(1);
                break;
            }
        }

        thread::yield_now();
    }
}
