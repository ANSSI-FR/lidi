//! Worker that acquires multiplex access and then becomes a `crate::receive::client` worker

use crate::{protocol, receive, receive::client};
use std::{io::Write, os::fd::AsRawFd, thread};

pub(crate) fn start<C, ClientNew, ClientEnd, E>(
    receiver: &receive::Receiver<ClientNew, ClientEnd>,
) -> Result<(), receive::Error>
where
    C: Write + AsRawFd,
    ClientNew: Send + Sync + Fn(protocol::ClientId) -> Result<C, E>,
    ClientEnd: Send + Sync + Fn(C, bool),
    E: Into<receive::Error>,
{
    loop {
        let (client_id, recvq) = receiver.for_clients.recv()?;

        log::debug!("try to acquire multiplex access..");
        receiver.multiplex_control.wait();
        log::debug!("multiplex access acquired");

        let client_res = client::start(receiver, client_id, &recvq);

        receiver.multiplex_control.signal();

        if let Err(e) = client_res {
            log::error!("client {client_id:x}: send loop error: {e}");
        }

        thread::yield_now();
    }
}
