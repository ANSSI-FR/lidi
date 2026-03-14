//! Worker that manages active transfers queue and dispatch incoming [`crate::protocol`]
//! blocks to clients

use lidi_protocol as protocol;
#[cfg(feature = "heartbeat")]
use std::time;
use std::{collections::HashMap, thread};

#[allow(clippy::too_many_lines)]
pub fn start<ClientNew, ClientEnd>(
    receiver: &crate::Receiver<ClientNew, ClientEnd>,
) -> Result<(), crate::Error> {
    let mut active_transfers: HashMap<
        protocol::ClientId,
        crossbeam_channel::Sender<protocol::Block>,
    > = HashMap::new();
    let mut ended_transfers: HashMap<
        protocol::ClientId,
        crossbeam_channel::Sender<protocol::Block>,
    > = HashMap::new();

    #[cfg(feature = "heartbeat")]
    let mut last_heartbeat = time::Instant::now();

    loop {
        #[cfg(not(feature = "heartbeat"))]
        let block = receiver.for_dispatch.recv()?;
        #[cfg(feature = "heartbeat")]
        let block = match receiver.config.heartbeat.as_ref() {
            None => receiver.for_dispatch.recv()?,
            Some(hb_interval) => match receiver.for_dispatch.recv_timeout(*hb_interval) {
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    if last_heartbeat.elapsed() > *hb_interval {
                        #[cfg(feature = "prometheus")]
                        metrics::counter!("lidi_receive_heartbeat_missed").increment(1);
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
                    None,
                    protocol::BlockType::Abort,
                    &receiver.raptorq,
                    client_id,
                    None,
                )?;

                if let Err(e) = client_sendq.try_send(block) {
                    #[cfg(feature = "prometheus")]
                    metrics::counter!("lidi_receive_client_queue_full").increment(1);
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
                #[cfg(feature = "heartbeat")]
                {
                    log::debug!("heartbeat received");
                    last_heartbeat = time::Instant::now();
                }
                continue;
            }
            protocol::BlockType::Start => {
                let payload = block.payload();
                match protocol::EndpointId::deserialize(payload) {
                    None => {
                        log::error!("client {client_id:x} for invalid endpoint");
                        continue;
                    }
                    Some(endpoint) => {
                        let (client_sendq, client_recvq) = if 0 < receiver.config.queue_size {
                            crossbeam_channel::bounded::<protocol::Block>(
                                receiver.config.queue_size,
                            )
                        } else {
                            crossbeam_channel::unbounded::<protocol::Block>()
                        };
                        active_transfers.insert(client_id, client_sendq);
                        receiver
                            .to_clients
                            .send((endpoint, client_id, client_recvq))?;
                    }
                }
            }
            protocol::BlockType::Abort | protocol::BlockType::End => will_end = true,
            protocol::BlockType::Data => (),
        }

        let Some(client_sendq) = active_transfers.get(&client_id) else {
            #[cfg(feature = "prometheus")]
            metrics::counter!("lidi_receive_blocks_for_inactive_client").increment(1);
            log::debug!("receive data for inactive transfer {client_id:x}");
            continue;
        };

        if let Err(e) = client_sendq.try_send(block) {
            #[cfg(feature = "prometheus")]
            metrics::counter!("lidi_receive_client_queue_full").increment(1);
            log::error!("failed to send block to client {client_id:x}: {e}");
            active_transfers.remove(&client_id);
            continue;
        }

        if will_end {
            let client_sendq = active_transfers.remove(&client_id).ok_or_else(|| {
                crate::Error::Internal(format!("transfer {client_id} is not active"))
            })?;

            ended_transfers.retain(|client_id, client_sendq| {
                let retain = !client_sendq.is_empty();
                if !retain {
                    log::debug!("purging ended transfer of client {client_id:x}");
                }
                retain
            });

            ended_transfers.insert(client_id, client_sendq);

            #[cfg(feature = "prometheus")]
            #[allow(clippy::cast_precision_loss)]
            metrics::gauge!("lidi_receive_ended_transfers_retained")
                .set(ended_transfers.len() as f64);
        }

        thread::yield_now();
    }
}
