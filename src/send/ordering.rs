//! Worker that assigns an id to a future `RaptorQ` block

use crate::send;

pub fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    let mut id = 0;

    loop {
        let Some(block) = sender.for_ordering.recv()? else {
            sender.to_send.send(None)?;
            return Ok(());
        };

        sender.to_encoding.send(Some((id, block)))?;

        id = id.wrapping_add(1);
    }
}
