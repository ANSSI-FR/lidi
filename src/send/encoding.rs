//! Worker that encodes protocol blocks into `RaptorQ` packets

use crate::send;
use std::{sync::atomic, thread};

pub fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    loop {
        let Some((block_id, block)) = sender.for_encoding.recv()? else {
            sender.to_send.send(None)?;
            return Ok(());
        };

        let client_id = block.client_id();

        log::debug!("encoding block {block_id} for client {client_id:x}");

        let packets = sender.raptorq.encode(block_id, block.serialized());

        'inner: loop {
            let block_to_send = sender.block_to_send.load(atomic::Ordering::SeqCst);

            if block_to_send == block_id {
                sender.to_send.send(Some(packets))?;
                sender.block_to_send.fetch_add(1, atomic::Ordering::SeqCst);
                break 'inner;
            }

            thread::yield_now();
        }
    }
}
