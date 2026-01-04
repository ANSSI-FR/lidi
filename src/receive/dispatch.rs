//! Worker that manages active transfers queue and dispatch incoming [`crate::protocol`]
//! blocks to clients

use crate::{protocol, receive};
use std::{collections::HashMap, thread, time};

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
    let mut active_transfers: HashMap<
        protocol::ClientId,
        crossbeam_channel::Sender<protocol::Block>,
    > = HashMap::new();
    let mut ended_transfers: HashMap<
        protocol::ClientId,
        crossbeam_channel::Sender<protocol::Block>,
    > = HashMap::new();

    let mut last_heartbeat = time::Instant::now();

    loop {
        let block = match receiver.config.heartbeat_interval.as_ref() {
            None => receiver.for_dispatch.recv()?,
            Some(hb_interval) => match receiver.for_dispatch.recv_timeout(*hb_interval) {
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    if last_heartbeat.elapsed() > *hb_interval {
                        log::warn!(
                            "no heartbeat block received for {} second(s)",
                            hb_interval.as_secs()
                        );
                    }
                    continue;
                }
                other => other?,
            },
        };

        let Some(block) = block else {
            // Synchonization has been lost
            // Marking all active transfers as failed
            for (client_id, client_sendq) in active_transfers {
                let block = protocol::Block::new(
                    protocol::BlockType::Abort,
                    &receiver.raptorq,
                    client_id,
                    None,
                )?;

                if let Err(e) = client_sendq.send(block) {
                    log::error!("failed to send payload to client {client_id:x}: {e}");
                }
            }
            active_transfers = HashMap::new();
            continue;
        };

        log::trace!("received {block}");

        let block_type = match block.block_type() {
            Err(e) => {
                log::error!("block of UNKNOWN type received ({e}), dropping it");
                continue;
            }
            Ok(mt) => mt,
        };

        let client_id = block.client_id();

        let mut will_end = false;

        match block_type {
            protocol::BlockType::Heartbeat => {
                log::debug!("heartbeat received");
                last_heartbeat = time::Instant::now();
                continue;
            }
            protocol::BlockType::Start => {
                let (client_sendq, client_recvq) =
                    crossbeam_channel::unbounded::<protocol::Block>();
                active_transfers.insert(client_id, client_sendq);
                receiver.to_clients.send((client_id, client_recvq))?;
            }
            protocol::BlockType::Abort | protocol::BlockType::End => will_end = true,
            protocol::BlockType::Data => (),
        }

        let Some(client_sendq) = active_transfers.get(&client_id) else {
            log::debug!("receive data for inactive transfer {client_id:x}");
            continue;
        };

        if let Err(e) = client_sendq.send(block) {
            log::error!("failed to send block to client {client_id:x}: {e}");
            active_transfers.remove(&client_id);
            continue;
        }

        if will_end {
            let client_sendq = active_transfers
                .remove(&client_id)
                .ok_or(receive::Error::Other(format!(
                    "transfer {client_id} is not active"
                )))?;

            ended_transfers.retain(|client_id, client_sendq| {
                let retain = !client_sendq.is_empty();
                if !retain {
                    log::debug!("purging ended transfer of client {client_id:x}");
                }
                retain
            });

            ended_transfers.insert(client_id, client_sendq);
        }

        thread::yield_now();
    }
}
