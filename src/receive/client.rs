//! Worker that writes decoded and reordered messages to client

use crate::{protocol, receive};
use std::{
    io::{self, Write},
    os::fd::AsRawFd,
    thread,
};

pub(crate) fn start<C, ClientNew, ClientEnd, E>(
    receiver: &receive::Receiver<ClientNew, ClientEnd>,
    client_id: protocol::ClientId,
    recvq: &crossbeam_channel::Receiver<protocol::Block>,
) -> Result<(), receive::Error>
where
    C: Write + AsRawFd,
    ClientNew: Send + Sync + Fn(protocol::ClientId) -> Result<C, E>,
    ClientEnd: Send + Sync + Fn(C, bool),
    E: Into<receive::Error>,
{
    log::info!("client {client_id:x}: starting transfer");

    let client = (receiver.client_new)(client_id).map_err(Into::into)?;
    let mut client =
        io::BufWriter::with_capacity(protocol::Block::max_data_len(&receiver.raptorq), client);

    let mut transmitted = 0;

    loop {
        let block = if let Some(timeout) = receiver.config.abort_timeout {
            recvq.recv_timeout(timeout).map_err(receive::Error::from)?
        } else {
            recvq.recv().map_err(receive::Error::from)?
        };

        let block_type = block.block_type()?;

        let payload = block.payload();

        if !payload.is_empty() {
            log::trace!("client {client_id:x}: payload {} bytes", payload.len());
            transmitted += payload.len();
            client.write_all(payload)?;
            if receiver.config.flush {
                client.flush()?;
            }
        }

        match block_type {
            protocol::BlockType::Abort => {
                log::warn!("client {client_id:x}: aborting transfer");
                (receiver.client_end)(
                    client.into_inner().map_err(|e| {
                        receive::Error::Other(format!("failed to retrieve client inner: {e}",))
                    })?,
                    false,
                );
                return Ok(());
            }
            protocol::BlockType::End => {
                log::info!(
                    "client {client_id:x}: finished transfer, {transmitted} bytes transmitted"
                );
                client.flush()?;
                (receiver.client_end)(
                    client.into_inner().map_err(|e| {
                        receive::Error::Other(format!("failed to retrieve client inner: {e}",))
                    })?,
                    true,
                );
                return Ok(());
            }
            _ => (),
        }

        thread::yield_now();
    }
}
