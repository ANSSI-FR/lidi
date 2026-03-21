//! Worker that reads data from a client socket and split it into [`crate::protocol`] blocks

use lidi_command_utils::config;
use lidi_protocol as protocol;
use std::{io, os::fd::AsRawFd, sync};

pub fn start<C>(
    sender: &crate::Sender<C>,
    endpoint_id: protocol::EndpointId,
    endpoint_options: config::EndpointOptions,
    client_id: protocol::ClientId,
    mut client: C,
) -> Result<(), crate::Error>
where
    C: io::Read + AsRawFd + Send,
{
    log::info!("client {client_id:x}: connected");

    let block_id = sender
        .next_block
        .fetch_add(1, sync::atomic::Ordering::SeqCst);

    sender.to_udp.send(Some(protocol::Block::new(
        sender.block_recycler.steal().success(),
        block_id,
        protocol::BlockType::Start,
        &sender.raptorq,
        client_id,
        Some(&endpoint_id.serialize()),
    )?))?;

    let mut buffer = vec![0; protocol::Block::max_data_len(&sender.raptorq)];
    let mut cursor = 0;
    let mut transmitted = 0;

    #[cfg(feature = "hash")]
    let mut hasher = if endpoint_options.hash {
        Some(lidi_command_utils::hash::StreamHasher::default())
    } else {
        None
    };

    loop {
        log::trace!("client {client_id:x}: read...");

        let read = client.read(&mut buffer[cursor..])?;

        if 0 < read {
            log::trace!("client {client_id:x}: {read} bytes read");
            cursor += read;

            if !(endpoint_options.flush || cursor >= buffer.len()) {
                continue;
            }
        }

        let block_type = if 0 == read {
            protocol::BlockType::End
        } else {
            protocol::BlockType::Data
        };

        log::trace!("client {client_id:x}: send {cursor} bytes");

        #[cfg(feature = "hash")]
        if let Some(hasher) = hasher.as_mut() {
            hasher.update(&buffer[..cursor]);
        }

        let block_id = sender
            .next_block
            .fetch_add(1, sync::atomic::Ordering::SeqCst);

        sender.to_udp.send(Some(protocol::Block::new(
            sender.block_recycler.steal().success(),
            block_id,
            block_type,
            &sender.raptorq,
            client_id,
            Some(&buffer[..cursor]),
        )?))?;

        transmitted += cursor;
        cursor = 0;

        if 0 == read {
            #[cfg(feature = "hash")]
            if let Some(hasher) = hasher {
                let hash = hasher.finalize();
                log::info!(
                    "client {client_id:x}: disconnect, {transmitted} bytes sent, hash is {hash:x}"
                );
            } else {
                log::info!("client {client_id:x}: disconnect, {transmitted} bytes sent");
            }

            #[cfg(not(feature = "hash"))]
            log::info!("client {client_id:x}: disconnect, {transmitted} bytes sent");

            return Ok(());
        }
    }
}
