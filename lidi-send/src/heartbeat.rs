//! Optional worker that periodically inserts [`crate::protocol`] heartbeat block in the encoding queue

use lidi_protocol as protocol;
use std::{sync, thread};

pub fn start<C>(sender: &crate::Sender<C>) -> Result<(), crate::Error> {
    let duration = sender.config.heartbeat.ok_or_else(|| {
        crate::Error::Internal(String::from(
            "heartbeat thread launched but no duration defined",
        ))
    })?;

    loop {
        log::debug!("send heartbeat");

        let block_id = sender
            .next_block
            .fetch_add(1, sync::atomic::Ordering::SeqCst);

        sender.to_udp.send(Some((
            block_id,
            protocol::Block::new(
                sender.block_recycler.steal().success(),
                protocol::BlockType::Heartbeat,
                &sender.raptorq,
                0,
                None,
            )?,
        )))?;

        thread::sleep(duration);
    }
}
