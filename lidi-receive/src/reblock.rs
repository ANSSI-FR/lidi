//! Worker for grouping packets according to their block numbers to handle potential UDP packets
//! reordering

use std::{array, mem, thread};

pub const WINDOW_WIDTH: u8 = u8::MAX / 2;

struct Block {
    ignore: bool,
    packets: Vec<raptorq::EncodingPacket>,
}

#[allow(clippy::too_many_lines)]
pub fn start<ClientNew, ClientEnd>(
    receiver: &crate::Receiver<ClientNew, ClientEnd>,
) -> Result<(), crate::Error> {
    let min_nb_packets = usize::from(receiver.raptorq.min_nb_packets());
    let nb_packets = usize::try_from(receiver.raptorq.nb_packets())
        .map_err(|e| crate::Error::Internal(format!("nb_packets: {e}")))?;

    let mut blocks: [_; u8::MAX as usize + 1] = array::from_fn(|_| Block {
        ignore: true,
        packets: Vec::with_capacity(nb_packets),
    });

    let mut cur_id: u8 = 0;

    let mut reset = true;

    loop {
        let packets = match receiver
            .for_reblock
            .recv_timeout(receiver.config.reset_timeout)
        {
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                reset = true;

                let damaged = blocks
                    .iter()
                    .filter(|block| !block.ignore)
                    .map(|block| block.packets.len() as u64)
                    .sum();

                if 0u64 < damaged {
                    #[cfg(feature = "prometheus")]
                    metrics::counter!("lidi_receive_blocks_damaged").increment(damaged);
                    log::error!("non empty block after timeout");
                    receiver.to_decode.send(super::Reassembled::Error)?;
                }

                continue;
            }
            Err(e) => return Err(crate::Error::from(e)),
            Ok(packets) => packets,
        };

        if reset {
            reset = false;

            for block in &mut blocks {
                block.ignore = true;
                block.packets.clear();
            }

            #[cfg(not(feature = "receive-mmsg"))]
            let first_packet = &packets;
            #[cfg(feature = "receive-mmsg")]
            let first_packet = &packets[0];

            cur_id = first_packet.payload_id().source_block_number();

            let mut id = cur_id;
            let last = id.wrapping_add(WINDOW_WIDTH);
            while id != last {
                blocks[id as usize].ignore = false;
                id = id.wrapping_add(1);
            }
        }

        #[cfg(not(feature = "receive-mmsg"))]
        {
            let id = packets.payload_id().source_block_number() as usize;

            if blocks[id].ignore {
                #[cfg(feature = "prometheus")]
                metrics::counter!("lidi_receive_packets_ignored").increment(1);
            } else {
                blocks[id].packets.push(packets);
            }
        }
        #[cfg(feature = "receive-mmsg")]
        for packet in packets {
            let id = packet.payload_id().source_block_number() as usize;

            if blocks[id].ignore {
                #[cfg(feature = "prometheus")]
                metrics::counter!("lidi_receive_packets_ignored").increment(1);
            } else {
                blocks[id].packets.push(packet);
            }
        }

        while blocks[cur_id as usize].packets.len() >= min_nb_packets {
            blocks[cur_id as usize].ignore = true;

            let packets = mem::replace(
                &mut blocks[cur_id as usize].packets,
                Vec::with_capacity(nb_packets),
            );

            #[cfg(feature = "prometheus")]
            metrics::counter!("lidi_receive_blocks_reassembled").increment(1);

            log::trace!("reassembled block {cur_id}");

            receiver.to_decode.send(super::Reassembled::Block {
                id: cur_id,
                packets,
            })?;

            let opposite = cur_id.wrapping_add(WINDOW_WIDTH) as usize;

            if !blocks[opposite].packets.is_empty() {
                #[cfg(feature = "prometheus")]
                metrics::counter!("lidi_receive_blocks_lost").increment(1);
                log::error!("lost block {opposite} (too far)");
                receiver.to_decode.send(super::Reassembled::Error)?;
                reset = true;
                break;
            }

            blocks[opposite].ignore = false;

            cur_id = cur_id.wrapping_add(1);
        }

        thread::yield_now();
    }
}
