//! Optional worker that periodically inserts [crate::protocol] heartbeat message in the encoding queue

use crate::{protocol, send};

pub(crate) fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    let alarm =
        crossbeam_channel::tick(sender.config.heartbeat_interval.expect("heartbeat enabled"));

    loop {
        sender.to_encoding.send(protocol::Message::new(
            protocol::MessageType::Heartbeat,
            sender.from_buffer_size,
            0,
            None,
        ))?;
        let _ = alarm.recv()?;
    }
}
