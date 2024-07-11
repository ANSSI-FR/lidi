//! Worker that manages active transfers queue and dispatch incoming [crate::protocol]
//! messages to clients

use crate::{protocol, receive};
use std::{
    collections::{BTreeMap, BTreeSet},
    time,
};

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
    let mut active_transfers: BTreeMap<
        protocol::ClientId,
        crossbeam_channel::Sender<protocol::Message>,
    > = BTreeMap::new();
    let mut ended_transfers: BTreeMap<
        protocol::ClientId,
        crossbeam_channel::Sender<protocol::Message>,
    > = BTreeMap::new();
    let mut failed_transfers: BTreeSet<protocol::ClientId> = BTreeSet::new();

    let mut last_heartbeat = time::Instant::now();

    loop {
        let message = if let Some(hb_interval) = receiver.config.heartbeat_interval {
            match receiver.for_dispatch.recv_timeout(hb_interval) {
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    if last_heartbeat.elapsed() > hb_interval {
                        log::warn!(
                            "no heartbeat message received during the last {} second(s)",
                            hb_interval.as_secs()
                        );
                    }
                    continue;
                }
                other => other?,
            }
        } else {
            receiver.for_dispatch.recv()?
        };

        let message = match message {
            Some(m) => m,
            None => {
                // Synchonization has been lost
                // Marking all active transfers as failed
                for (client_id, client_sendq) in active_transfers {
                    let message = protocol::Message::new(
                        protocol::MessageType::Abort,
                        receiver.to_buffer_size as u32,
                        client_id,
                        None,
                    );

                    if let Err(e) = client_sendq.send(message) {
                        log::error!("failed to send payload to client {client_id:x}: {e}");
                    }

                    failed_transfers.insert(client_id);
                    ended_transfers.insert(client_id, client_sendq);
                }
                active_transfers = BTreeMap::new();
                continue;
            }
        };

        log::trace!("received {message}");

        let client_id = message.client_id();

        if failed_transfers.contains(&client_id) {
            continue;
        }

        let message_type =
            match message.message_type() {
                Err(e) => {
                    log::error!("message of UNKNOWN type received ({e}), dropping it");
                    continue;
                }
                Ok(mt) => mt,
            };

        let mut will_end = false;

        match message_type {
            protocol::MessageType::Heartbeat => {
                last_heartbeat = time::Instant::now();
                continue;
            }

            protocol::MessageType::Start => {
                let (client_sendq, client_recvq) =
                    crossbeam_channel::unbounded::<protocol::Message>();

                active_transfers.insert(client_id, client_sendq);

                receiver.to_clients.send((client_id, client_recvq))?;
            }

            protocol::MessageType::Abort | protocol::MessageType::End => will_end = true,

            protocol::MessageType::Data => (),
        }

        match active_transfers.get(&client_id) {
            None => {
                log::error!("receive data for inactive transfer {client_id:x}");
                failed_transfers.insert(client_id);
            }
            Some(client_sendq) => {
                if let Err(e) = client_sendq.send(message) {
                    log::error!("failed to send payload to client {client_id:x}: {e}");
                    active_transfers.remove(&client_id);
                    failed_transfers.insert(client_id);
                    continue;
                }

                if will_end {
                    let client_sendq = active_transfers
                        .remove(&client_id)
                        .expect("active transfer");

                    ended_transfers.retain(|client_id, client_sendq| {
                        let retain = !client_sendq.is_empty();
                        if !retain {
                            log::debug!("purging ended transfer of client {client_id:x}");
                        }
                        retain
                    });

                    ended_transfers.insert(client_id, client_sendq);
                }
            }
        }
    }
}
