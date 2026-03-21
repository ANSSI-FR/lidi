//! Worker that acquires multiplex access and then becomes a `crate::receive::client` worker

use crate::client;
use lidi_command_utils::config;
use lidi_protocol as protocol;
use std::{io::Write, os::fd::AsRawFd, thread};

pub fn start<C, ClientNew, ClientEnd, E>(
    receiver: &crate::Receiver<ClientNew, ClientEnd>,
) -> Result<(), crate::Error>
where
    C: Write + AsRawFd,
    ClientNew: Send + Sync + Fn(&config::Endpoint, protocol::ClientId) -> Result<C, E>,
    ClientEnd: Send + Sync + Fn(C, bool),
    E: Into<crate::Error>,
{
    loop {
        let (endpoint_id, client_id, recvq) = receiver.for_clients.recv()?;

        let client_res = client::start(receiver, endpoint_id, client_id, &recvq);

        if let Err(e) = client_res {
            log::error!("client {client_id:x}: {e}");
        }

        thread::yield_now();
    }
}
