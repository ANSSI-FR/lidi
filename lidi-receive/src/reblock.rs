//! Worker for grouping packets according to their block numbers to handle potential UDP packets
//! reordering

use std::{array, thread};

pub const WINDOW_WIDTH: u8 = u8::MAX / 2;

struct Block {
    ignore: bool,
    packets: Vec<raptorq::EncodingPacket>,
}

fn send_to_decode(
    to_decode: &crossbeam_channel::Sender<super::Reassembled>,
    id: u8,
    blocks: &mut [Block],
) -> Result<bool, crate::Error> {
    blocks[id as usize].ignore = true;

    let packets = blocks[id as usize].packets.clone();
    blocks[id as usize].packets.clear();

    to_decode.send(super::Reassembled::Block { id, packets })?;

    #[cfg(feature = "prometheus")]
    metrics::counter!("lidi_receive_blocks_reassembled").increment(1);

    log::trace!("reassembled block {id}");

    let opposite = id.wrapping_add(WINDOW_WIDTH) as usize;

    if blocks[opposite].ignore {
        blocks[opposite].ignore = false;

        if !blocks[opposite].packets.is_empty() {
            #[cfg(feature = "prometheus")]
            metrics::counter!("lidi_receive_blocks_lost").increment(1);
            log::error!("lost block {opposite} (too far)");
            to_decode.send(super::Reassembled::Error)?;
            return Ok(true);
        }
    }

    Ok(false)
}

pub fn start<ClientNew, ClientEnd>(
    receiver: &crate::Receiver<ClientNew, ClientEnd>,
) -> Result<(), crate::Error> {
    let min_nb_packets = usize::try_from(receiver.raptorq.min_nb_packets())
        .map_err(|e| crate::Error::Internal(format!("min_nb_packets: {e}")))?;
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
                if !reset {
                    reset = true;

                    let prev = cur_id.wrapping_sub(1);
                    while cur_id != prev {
                        let nb_packets = blocks[cur_id as usize].packets.len();
                        if 0 < nb_packets {
                            if nb_packets < min_nb_packets {
                                log::warn!(
                                    "block {cur_id} is incomplete ({nb_packets} packets) after reset timeout, forcibly send to decode"
                                );
                            }
                            let _ = send_to_decode(&receiver.to_decode, cur_id, &mut blocks)?;
                        }
                        cur_id = cur_id.wrapping_add(1);
                    }
                }

                continue;
            }
            Err(e) => return Err(crate::Error::from(e)),
            Ok(packets) => {
                #[cfg(not(feature = "receive-mmsg"))]
                {
                    [packets]
                }
                #[cfg(feature = "receive-mmsg")]
                packets
            }
        };

        if reset {
            reset = false;

            for block in &mut blocks {
                block.ignore = true;
                block.packets.clear();
            }

            let first_packet = &packets[0];

            cur_id = first_packet.payload_id().source_block_number();

            let mut id = cur_id;
            let last = id.wrapping_add(WINDOW_WIDTH);
            while id != last {
                blocks[id as usize].ignore = false;
                id = id.wrapping_add(1);
            }
        }

        let mut fast_track = false;

        for packet in packets {
            let id = packet.payload_id().source_block_number();

            if blocks[id as usize].ignore {
                if id == cur_id.wrapping_add(WINDOW_WIDTH) {
                    fast_track = true;
                    blocks[id as usize].ignore = false;
                    blocks[id as usize].packets.push(packet);
                } else {
                    #[cfg(feature = "prometheus")]
                    metrics::counter!("lidi_receive_packets_ignored").increment(1);
                }
            } else {
                blocks[id as usize].packets.push(packet);
            }
        }

        if fast_track {
            log::warn!("probable network interrupt, fast track first block");
            let _ = send_to_decode(&receiver.to_decode, cur_id, &mut blocks)?;
            cur_id = cur_id.wrapping_add(1);
        }

        while blocks[cur_id as usize].packets.len() >= min_nb_packets {
            reset = send_to_decode(&receiver.to_decode, cur_id, &mut blocks)?;
            cur_id = cur_id.wrapping_add(1);
        }

        thread::yield_now();
    }
}
