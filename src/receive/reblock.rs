//! Worker for grouping packets according to their block numbers to handle potential UDP packets
//! reordering

use crate::receive;
use std::{mem, thread};

pub const WINDOW_WIDTH: u8 = u8::MAX / 2;

#[allow(clippy::too_many_lines)]
pub fn start<ClientNew, ClientEnd>(
    receiver: &receive::Receiver<ClientNew, ClientEnd>,
) -> Result<(), receive::Error> {
    let min_nb_packets = usize::from(receiver.raptorq.min_nb_packets());
    let nb_packets = usize::try_from(receiver.raptorq.nb_packets())
        .map_err(|e| receive::Error::Other(format!("nb_packets: {e}")))?;

    let mut blocks_data = vec![Vec::with_capacity(nb_packets); u8::MAX as usize + 1];
    let mut blocks_ignore = [true; u8::MAX as usize + 1];

    let mut cur_id: u8 = 0;

    let mut reset = true;

    loop {
        let packets = match receiver
            .for_reblock
            .recv_timeout(receiver.config.reset_timeout)
        {
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                reset = true;

                let mut damaged = false;

                for id in 0..=u8::MAX as usize {
                    if blocks_ignore[id] && !blocks_data[id].is_empty() {
                        damaged = true;
                    }
                }

                if damaged {
                    log::error!("non empty block after timeout");
                    receiver.to_decode.send(super::Reassembled::Error)?;
                }

                continue;
            }
            Err(e) => return Err(receive::Error::from(e)),
            Ok(packets) => packets,
        };

        if reset {
            reset = false;

            for block in &mut blocks_data {
                block.clear();
            }
            blocks_ignore.fill(true);

            #[cfg(not(feature = "receive-mmsg"))]
            let first_packet = &packets;
            #[cfg(feature = "receive-mmsg")]
            let first_packet = &packets[0];

            cur_id = first_packet.payload_id().source_block_number();

            let mut id = cur_id;
            let last = id.wrapping_add(WINDOW_WIDTH);
            while id != last {
                blocks_ignore[usize::from(id)] = false;
                id = id.wrapping_add(1);
            }
        }

        #[cfg(not(feature = "receive-mmsg"))]
        {
            let id = packets.payload_id().source_block_number() as usize;

            if !blocks_ignore[id] {
                blocks_data[id].push(packets);
            }
        }
        #[cfg(feature = "receive-mmsg")]
        for packet in packets {
            let id = packet.payload_id().source_block_number() as usize;

            if !blocks_ignore[id] {
                blocks_data[id].push(packet);
            }
        }

        while blocks_data[cur_id as usize].len() >= min_nb_packets {
            blocks_ignore[cur_id as usize] = true;

            let packets = mem::replace(
                &mut blocks_data[cur_id as usize],
                Vec::with_capacity(nb_packets),
            );

            log::trace!("reassembled block {cur_id}");

            receiver.to_decode.send(super::Reassembled::Block {
                id: cur_id,
                packets,
            })?;

            let opposite = cur_id.wrapping_add(WINDOW_WIDTH) as usize;

            if !blocks_data[opposite].is_empty() {
                log::error!("lost block {opposite} (too far)");
                receiver.to_decode.send(super::Reassembled::Error)?;
                reset = true;
                break;
            }

            blocks_ignore[opposite] = false;

            cur_id = cur_id.wrapping_add(1);
        }

        thread::yield_now();
    }
}
