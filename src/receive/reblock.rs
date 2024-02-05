//! Worker for grouping packets according to their block numbers to handle potential UDP packets
//! reordering

use crate::{protocol, receive};
use std::collections::HashMap;

const BLOCK_GAP_WARNING_THRESHOLD: u8 = 2;

// Discards queues in Hashmap queues_by_block_id, where block_id is not between
// (leading_block_id - window_size) and (leading_block_id).
// Raises warnings for each discarded queue containing partial data (len < nb_normal_packets)
fn discard_queues_outside_retention_window(
    leading_block_id: u8,
    window_size: u8,
    queues_by_block_id: &mut HashMap<u8, Vec<raptorq::EncodingPacket>>,
    nb_normal_packets: usize,
) {
    queues_by_block_id.retain(|&k, v| {
        let retain = is_in_retention_window(k, leading_block_id, window_size);
        if !retain && (v.len() < nb_normal_packets) {
            log::warn!("discarding incomplete block {k} (currently on block {leading_block_id})")
        }
        retain
    });
}

// Returns true if block_id is between (leading_block_id - window_size) and
// leading_block_id; otherwise, returns false.
fn is_in_retention_window(block_id: u8, leading_block_id: u8, window_size: u8) -> bool {
    is_in_wrapped_interval(
        block_id,
        (
            leading_block_id.wrapping_sub(window_size - 1),
            leading_block_id,
        ),
    )
}

// Returns true if value is between (leading_block_id - window_size) and
// leading_block_id; otherwise, returns false.
fn is_in_wrapped_interval(value: u8, interval: (u8, u8)) -> bool {
    let (lower_bound, upper_bound) = interval;
    if lower_bound < upper_bound {
        // continuous interval like within 32-48
        lower_bound <= value && value <= upper_bound
    } else {
        // wrapped interval like (0-8 or 248-255)
        value <= upper_bound || lower_bound <= value
    }
}

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
    let nb_normal_packets = protocol::nb_encoding_packets(&receiver.object_transmission_info);
    let nb_repair_packets = protocol::nb_repair_packets(
        &receiver.object_transmission_info,
        receiver.config.repair_block_size,
    );
    let reblock_retention_window = receiver.config.reblock_retention_window;

    let mut next_block_id_overwrites_leading = true;

    let capacity = nb_normal_packets as usize + nb_repair_packets as usize;

    let mut leading_block_id: u8 = 0;
    let mut next_sendable_block_id: u8 = 0;
    let mut queues_by_block_id: HashMap<u8, Vec<raptorq::EncodingPacket>> =
        HashMap::with_capacity(reblock_retention_window as usize);

    loop {
        let packets = match receiver
            .for_reblock
            .recv_timeout(receiver.config.flush_timeout)
        {
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                log::trace!("timeout while waiting for next packets");
                queues_by_block_id.clear();
                next_block_id_overwrites_leading = true;
                continue;
            }
            Err(e) => return Err(receive::Error::from(e)),
            Ok(packet) => packet,
        };

        for packet in packets {
            let payload_id = packet.payload_id();
            let packet_block_id = payload_id.source_block_number();

            if next_block_id_overwrites_leading {
                log::debug!("new leading block id: ({packet_block_id})");
                leading_block_id = packet_block_id;
                next_sendable_block_id = packet_block_id;
                next_block_id_overwrites_leading = false;
            } else if !is_in_retention_window(
                packet_block_id,
                leading_block_id,
                reblock_retention_window,
            ) {
                log::debug!("new leading block id: {packet_block_id} (was {leading_block_id})");
                if !is_in_wrapped_interval(
                    packet_block_id,
                    (
                        leading_block_id,
                        leading_block_id + BLOCK_GAP_WARNING_THRESHOLD,
                    ),
                ) {
                    log::warn!("large gap in block sequence (received {packet_block_id} while on block {leading_block_id})")
                }
                leading_block_id = packet_block_id;

                log::debug!("discarding all packets for blocks outside new retention window");
                discard_queues_outside_retention_window(
                    leading_block_id,
                    reblock_retention_window,
                    &mut queues_by_block_id,
                    nb_normal_packets as usize,
                );

                // update next_sendable_block_id if now outside window
                if !is_in_retention_window(
                    next_sendable_block_id,
                    leading_block_id,
                    reblock_retention_window,
                ) {
                    next_sendable_block_id =
                        leading_block_id.wrapping_sub(reblock_retention_window - 1);
                    log::debug!("bumped next_sendable_block_id to {next_sendable_block_id}");

                    // check sendable queues
                    loop {
                        let queue = queues_by_block_id
                            .entry(next_sendable_block_id)
                            .or_insert(Vec::with_capacity(capacity));
                        let qlen = queue.len();
                        let queue_packet_id = next_sendable_block_id;

                        if (nb_normal_packets as usize) > qlen {
                            break;
                        }
                        log::debug!("trying to decode block {queue_packet_id} with {qlen} packets");
                        receiver
                            .to_decoding
                            .send((queue_packet_id, queue.to_vec()))?;
                        next_sendable_block_id = next_sendable_block_id.wrapping_add(1);
                    }
                }
            }

            // push packet into queue
            let mut queue = queues_by_block_id
                .entry(packet_block_id)
                .or_insert(Vec::with_capacity(capacity));
            let mut qlen = queue.len();
            let mut queue_packet_id = packet_block_id;

            log::trace!("queueing packet for block {packet_block_id}");
            queue.push(packet);

            // send block if enough packets
            while nb_normal_packets as usize == qlen {
                if next_sendable_block_id != queue_packet_id {
                    log::debug!("ready to decode block {queue_packet_id} with {qlen} packets, but still waiting on block {next_sendable_block_id}");
                    break;
                }

                log::debug!("trying to decode block {queue_packet_id} with {qlen} packets");
                receiver
                    .to_decoding
                    .send((queue_packet_id, queue.to_vec()))?;

                next_sendable_block_id = next_sendable_block_id.wrapping_add(1);

                // check if next queue is ready to send
                queue = queues_by_block_id
                    .entry(next_sendable_block_id)
                    .or_insert(Vec::with_capacity(capacity));
                qlen = queue.len();
                queue_packet_id = next_sendable_block_id;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_in_wrapped_interval;

    #[test]
    fn test_is_in_wrapped_interval() {
        for (value, lower_bound, upper_bound, expected_result) in vec![
            (0, 0, 0, true),
            (0, 0, 255, true),
            // continuous interval
            (29, 30, 40, false),
            (30, 30, 40, true),
            (31, 30, 40, true),
            (40, 30, 40, true),
            (41, 30, 40, false),
            // wrapping
            (29, 40, 30, true),
            (30, 40, 30, true),
            (31, 40, 30, false),
            (40, 40, 30, true),
            (41, 40, 30, true),
            // edge cases
            (0, 255, 0, true),
            (3, 255, 0, false),
            (255, 255, 0, true),
        ] {
            let res = is_in_wrapped_interval(value, (lower_bound, upper_bound));
            assert_eq!(expected_result, res, "expected {expected_result}; got {res}. value: {value}; lower_bound: {lower_bound}; upper_bound: {upper_bound}");
        }
    }
}
