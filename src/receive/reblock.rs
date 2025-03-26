//! Worker for grouping packets according to their block numbers to handle potential UDP packets
//! reordering

use crate::{protocol, receive};

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
    let nb_normal_packets = protocol::nb_encoding_packets(&receiver.object_transmission_info);
    let nb_repair_packets = protocol::nb_repair_packets(
        &receiver.object_transmission_info,
        receiver.config.repair_block_size,
    );

    let mut desynchro = true;
    let capacity = nb_normal_packets as usize + nb_repair_packets as usize;
    let mut prev_queue: Option<Vec<raptorq::EncodingPacket>> = None;
    let mut queue = Vec::with_capacity(capacity);
    let mut block_id = 0;

    loop {
        let packets = match receiver
            .for_reblock
            .recv_timeout(receiver.config.flush_timeout)
        {
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                let qlen = queue.len();
                if 0 < qlen {
                    // no more traffic but ongoing block, trying to decode
                    if nb_normal_packets as usize <= qlen {
                        log::debug!("flushing block {block_id} with {qlen} packets");
                        receiver.to_decoding.send((block_id, Some(queue)))?;
                        block_id = block_id.wrapping_add(1);
                    } else {
                        log::debug!(
                            "not enough packets ({qlen} packets) to decode block {block_id}"
                        );
                        log::warn!("lost block {block_id}");
                        receiver.to_decoding.send((block_id, None))?;
                        desynchro = true;
                    }
                    queue = Vec::with_capacity(capacity);
                    prev_queue = None;
                } else {
                    // without data for some time we reset the current block_id
                    desynchro = true;
                }
                continue;
            }
            Err(e) => return Err(receive::Error::from(e)),
            Ok(packet) => packet,
        };

        for packet in packets {
            let payload_id = packet.payload_id();
            let message_block_id = payload_id.source_block_number();

            if desynchro {
                block_id = message_block_id;
                receiver.resync_needed_block_id.store((true, block_id));
                desynchro = false;
            }

            if message_block_id == block_id {
                log::trace!("queueing in block {block_id}");
                queue.push(packet);
                continue;
            }

            if message_block_id.wrapping_add(1) == block_id {
                //packet is from previous block; is this block parked ?
                if let Some(mut pqueue) = prev_queue {
                    pqueue.push(packet);
                    if nb_normal_packets as usize <= pqueue.len() {
                        //now there is enough packets to decode it
                        receiver
                            .to_decoding
                            .send((message_block_id, Some(pqueue)))?;
                        prev_queue = None;
                    } else {
                        prev_queue = Some(pqueue);
                    }
                }
                continue;
            }

            if message_block_id != block_id.wrapping_add(1) {
                log::warn!(
                    "discarding packet with block_id {message_block_id} (current block_id is {block_id})"
                );
                continue;
            }

            //this is the first packet of the next block

            if nb_normal_packets as usize <= queue.len() {
                //enough packets in the current block to decode it
                receiver.to_decoding.send((block_id, Some(queue)))?;
                if prev_queue.is_some() {
                    log::warn!("lost block {}", block_id.wrapping_sub(1));
                }
                prev_queue = None;
            } else {
                //not enough packet, parking the current block
                prev_queue = Some(queue);
            }

            //starting the next block

            block_id = message_block_id;

            log::trace!("queueing in block {block_id}");
            queue = Vec::with_capacity(capacity);
            queue.push(packet);
        }
    }
}
