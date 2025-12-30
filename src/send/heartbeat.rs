//! Optional worker that periodically inserts [`crate::protocol`] heartbeat block in the encoding queue

use crate::{protocol, send};
use std::thread;

pub(crate) fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    let Some(duration) = sender.config.heartbeat_interval else {
        return Err(send::Error::Other(
            "no heartbeat duration but heartbeat enabled".into(),
        ));
    };

    loop {
        if sender.broken_pipeline.load() {
            return Ok(());
        }

        log::debug!("send heartbeat");

        sender.to_encoding.send(protocol::Block::new(
            protocol::BlockType::Heartbeat,
            &sender.raptorq,
            0,
            None,
        )?)?;

        thread::sleep(duration);
    }
}
