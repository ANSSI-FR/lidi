//! Worker that reads data from a client socket and split it into [`crate::protocol`] blocks

use crate::{protocol, send};
#[cfg(feature = "transfer-hash")]
use fasthash::HasherExt;
#[cfg(feature = "transfer-hash")]
use std::hash::Hasher;
use std::{io, os::fd::AsRawFd, sync};

pub fn start<C>(
    sender: &send::Sender<C>,
    client_id: protocol::ClientId,
    mut client: C,
) -> Result<(), send::Error>
where
    C: io::Read + AsRawFd + Send,
{
    log::info!("client {client_id:x}: connected");

    let block_id = sender
        .next_block
        .fetch_add(1, sync::atomic::Ordering::SeqCst);

    sender.to_udp.send(Some((
        block_id,
        protocol::Block::new(protocol::BlockType::Start, &sender.raptorq, client_id, None)?,
    )))?;

    let mut buffer = vec![0; protocol::Block::max_data_len(&sender.raptorq)];
    let mut cursor = 0;
    let mut transmitted = 0;

    #[cfg(feature = "transfer-hash")]
    let mut hasher = if sender.config.hash {
        Some(fasthash::SpookyHasherExt::default())
    } else {
        None
    };

    loop {
        log::trace!("client {client_id:x}: read...");

        let read = client.read(&mut buffer[cursor..])?;

        if 0 < read {
            log::trace!("client {client_id:x}: {read} bytes read");
            cursor += read;

            if !(sender.config.flush || cursor >= buffer.len()) {
                continue;
            }
        }

        let block_type = if 0 == read {
            protocol::BlockType::End
        } else {
            protocol::BlockType::Data
        };

        log::trace!("client {client_id:x}: send {cursor} bytes");

        #[cfg(feature = "transfer-hash")]
        if let Some(hasher) = hasher.as_mut() {
            hasher.write(&buffer[..cursor]);
        }

        let block_id = sender
            .next_block
            .fetch_add(1, sync::atomic::Ordering::SeqCst);

        sender.to_udp.send(Some((
            block_id,
            protocol::Block::new(
                block_type,
                &sender.raptorq,
                client_id,
                Some(&buffer[..cursor]),
            )?,
        )))?;

        transmitted += cursor;
        cursor = 0;

        if 0 == read {
            #[cfg(feature = "transfer-hash")]
            if let Some(hasher) = hasher {
                let hash = hasher.finish_ext();
                log::info!(
                    "client {client_id:x}: disconnect, {transmitted} bytes sent, hash is {hash:x}"
                );
            } else {
                log::info!("client {client_id:x}: disconnect, {transmitted} bytes sent");
            }

            #[cfg(not(feature = "transfer-hash"))]
            log::info!("client {client_id:x}: disconnect, {transmitted} bytes sent");

            return Ok(());
        }
    }
}
