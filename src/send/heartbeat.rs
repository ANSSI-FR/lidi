//! Optional worker that periodically inserts [crate::protocol] heartbeat message in the encoding queue

use crate::{
    protocol::{Header, MessageType},
    send,
};
use std::io::Result;

pub(crate) fn start(_sender: &send::SenderConfig) -> Result<()> {
    //let alarm =
    //    crossbeam_channel::tick(sender.config.hearbeat_interval.expect("heartbeat enabled"));

    let _header = Header::new(MessageType::Heartbeat, 0, 0);

    /*
    loop {

        sender
            .to_encoding
            .send(message)
            .map_err(|e| Error::new(ErrorKind::BrokenPipe, format!("{e}")))?;
        let _ = alarm
            .recv()
            .map_err(|e| Error::new(ErrorKind::BrokenPipe, format!("{e}")))?;

        // XXX TODO send on udp directly
    }
        */

    Ok(())
}
