//! Worker that reorders received messages according to block numbers

use crate::receive;

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
    let mut block_to_receive = 0;
    let mut pending_messages = [const { None }; u8::MAX as usize + 1];

    loop {
        let (block_id, message) = receiver.for_reordering.recv()?;

        if message.is_none() {
            // Synchronization lost, dropping everything
            log::warn!("synchronization lost received, dropping everything, propagating it");
            pending_messages.fill_with(|| None);
            receiver.to_dispatch.send(None)?;
            continue;
        }

        let (resync_needed, resync_block_id) = receiver.resync_needed_block_id.take();

        if resync_needed {
            log::debug!("forced resynchronization, propagating it");
            receiver.to_dispatch.send(None)?;
            if pending_messages.iter().any(Option::is_some) {
                log::warn!("forced resynchronization with pending messages, dropping everything");
                pending_messages.fill_with(|| None);
            }
            block_to_receive = resync_block_id;
        }

        log::debug!("received block {block_id}, expecting block {block_to_receive}");

        if block_to_receive == block_id {
            let message = if pending_messages[block_to_receive as usize].is_some() {
                // a message was already pending
                // using the old one, storing the newly received one
                pending_messages[block_to_receive as usize]
                    .replace(message)
                    .expect("infallible")
            } else {
                // no message was pending, using the newly received one
                message
            };

            receiver.to_dispatch.send(message)?;
            block_to_receive = block_to_receive.wrapping_add(1);

            // flushing as much as possible further pending blocks
            while let Some(message) = pending_messages[block_to_receive as usize].take() {
                receiver.to_dispatch.send(message)?;
                block_to_receive = block_to_receive.wrapping_add(1);
            }
        } else if pending_messages[block_id as usize]
            .replace(message)
            .is_some()
        {
            log::error!(
                "received a new block {block_id} but existing one was not sent to dispatch, synchronization lost, dropping everything"
            );
            pending_messages.fill_with(|| None);
            receiver.to_dispatch.send(None)?;
        }
    }
}
