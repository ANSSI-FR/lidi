//! Worker that writes decoded and reordered messages to client

use lidi_protocol as protocol;
use lidi_utils::config;
use std::{
    io::{self, Write},
    os::fd::AsRawFd,
    thread,
};

pub fn start<C, ClientNew, ClientEnd, E>(
    receiver: &crate::Receiver<ClientNew, ClientEnd>,
    endpoint_id: protocol::EndpointId,
    client_id: protocol::ClientId,
    recvq: &crossbeam_channel::Receiver<protocol::Block>,
) -> Result<(), crate::Error>
where
    C: Write + AsRawFd,
    ClientNew: Send + Sync + Fn(&config::Endpoint, protocol::ClientId) -> Result<C, E>,
    ClientEnd: Send + Sync + Fn(C, bool),
    E: Into<crate::Error>,
{
    let Some(endpoint) = receiver.config.to.get(usize::from(endpoint_id.value())) else {
        return Err(crate::Error::Protocol(protocol::Error::InvalidEndpoint(
            endpoint_id,
        )));
    };

    log::info!("client {client_id:x}: starting transfer to endpoint {endpoint_id}");

    let client = (receiver.client_new)(endpoint, client_id).map_err(Into::into)?;
    let mut client =
        io::BufWriter::with_capacity(protocol::Block::max_data_len(&receiver.raptorq), client);

    let mut transmitted = 0;

    #[cfg(feature = "hash")]
    let mut hasher = if receiver.config.hash {
        Some(lidi_utils::hash::StreamHasher::default())
    } else {
        None
    };

    loop {
        let block = if let Some(timeout) = receiver.config.abort_timeout {
            recvq.recv_timeout(timeout).map_err(crate::Error::from)?
        } else {
            recvq.recv().map_err(crate::Error::from)?
        };

        let block_type = block.block_type()?;

        let payload = block.payload();

        if !payload.is_empty() {
            log::trace!("client {client_id:x}: payload {} bytes", payload.len());

            #[cfg(feature = "hash")]
            if let Some(hasher) = hasher.as_mut() {
                hasher.update(payload);
            }

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
                        crate::Error::Internal(format!("failed to retrieve client inner: {e}",))
                    })?,
                    false,
                );
                return Ok(());
            }
            protocol::BlockType::End => {
                #[cfg(feature = "hash")]
                if let Some(hasher) = hasher {
                    let hash = hasher.finalize();
                    log::info!(
                        "client {client_id:x}: finished transfer, {transmitted} bytes transmitted, hash is {hash:x}"
                    );
                } else {
                    log::info!(
                        "client {client_id:x}: finished transfer, {transmitted} bytes transmitted"
                    );
                }

                #[cfg(not(feature = "hash"))]
                log::info!(
                    "client {client_id:x}: finished transfer, {transmitted} bytes transmitted"
                );

                client.flush()?;
                (receiver.client_end)(
                    client.into_inner().map_err(|e| {
                        crate::Error::Internal(format!("failed to retrieve client inner: {e}",))
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
