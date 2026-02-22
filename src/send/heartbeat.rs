//! Optional worker that periodically inserts [`crate::protocol`] heartbeat block in the encoding queue

use crate::{protocol, send};
use std::{sync, thread};

pub fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    let Some(duration) = sender.config.heartbeat_interval else {
        return Err(send::Error::Other(String::from(
            "no heartbeat duration but heartbeat enabled",
        )));
    };

    loop {
        log::debug!("send heartbeat");

        let block_id = sender
            .next_block
            .fetch_add(1, sync::atomic::Ordering::SeqCst);

        sender.to_udp.send(Some((
            block_id,
            protocol::Block::new(protocol::BlockType::Heartbeat, &sender.raptorq, 0, None)?,
        )))?;

        thread::sleep(duration);
    }
}
