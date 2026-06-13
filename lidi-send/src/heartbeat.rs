//! Optional worker that periodically inserts [`crate::protocol`] heartbeat block in the encoding queue

use lidi_protocol as protocol;
use std::thread;

pub fn start<C>(sender: &crate::Sender<C>) -> Result<(), crate::Error> {
    let duration = sender.config.heartbeat.ok_or_else(|| {
        crate::Error::Internal(String::from(
            "heartbeat thread launched but no duration defined",
        ))
    })?;

    loop {
        log::debug!("send heartbeat");

        sender.to_encode.send(Some(protocol::Block::new(
            sender.block_recycler.steal().success(),
            protocol::BlockType::Heartbeat,
            &sender.raptorq,
            0,
            0,
            None,
        )?))?;

        thread::sleep(duration);
    }
}
