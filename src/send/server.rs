//! Worker that gets a client socket and becomes a `crate::send::client` worker

use crate::{protocol, send, send::client};
use std::{io::Read, os::fd::AsRawFd, thread};

pub(crate) fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error>
where
    C: Read + AsRawFd + Send,
{
    loop {
        let client = sender.for_server.recv()?;

        sender.multiplex_control.wait();

        let client_id = protocol::new_client_id();

        let client_res = client::start(sender, client_id, client);

        sender.multiplex_control.signal();

        if let Err(e) = client_res {
            log::error!("client {client_id:x}: error: {e}");

            if let Err(e) = sender.to_encoding.send(protocol::Block::new(
                protocol::BlockType::Abort,
                &sender.raptorq,
                client_id,
                None,
            )?) {
                log::error!("client {client_id:x}: failed to abort : {e}");
            }
        }

        thread::yield_now();
    }
}
