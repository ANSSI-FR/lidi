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
        let message = match receiver
            .for_dispatch
            .recv_timeout(receiver.config.heartbeat_interval)
        {
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if last_heartbeat.elapsed() > receiver.config.heartbeat_interval {
                    log::warn!(
                        "no heartbeat message received during the last {} second(s)",
                        receiver.config.heartbeat_interval.as_secs()
                    );
                }
                continue;
            }
            other => other?,
        };

        log::trace!("received {message}");

        let client_id = message.client_id();

        if failed_transfers.contains(&client_id) {
            continue;
        }

        let message_type = message.message_type()?;

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

        log::trace!("message = {message}");

        let client_sendq = active_transfers.get(&client_id).expect("active transfer");

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
