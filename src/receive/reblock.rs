//! Worker for grouping packets according to their block numbers to handle potential UDP packets
//! reordering

use crate::{receive, udp};
use std::{mem, thread};

pub(crate) const WINDOW_WIDTH: u8 = u8::MAX / 2;

pub(crate) fn start<ClientNew, ClientEnd>(
    receiver: &receive::Receiver<ClientNew, ClientEnd>,
) -> Result<(), receive::Error> {
    let min_nb_packets = usize::from(receiver.raptorq.min_nb_packets());
    let nb_packets = usize::try_from(receiver.raptorq.nb_packets())
        .map_err(|e| receive::Error::Other(format!("nb_packets: {e}")))?;

    let mut blocks_data = vec![Vec::with_capacity(nb_packets); usize::from(u8::MAX) + 1];
    let mut blocks_ignore = vec![true; usize::from(u8::MAX) + 1];

    let mut cur_id: u8 = 0;

    let mut reset = true;

    loop {
        let datagrams = match receiver
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
            Ok(datagrams) => datagrams,
        };

        if reset {
            reset = false;

            for block in &mut blocks_data {
                block.clear();
            }
            blocks_ignore.fill(true);

            let first_datagram = match &datagrams {
                udp::Datagrams::Single(datagram) => datagram,
                udp::Datagrams::Multiple(datagrams) => &datagrams[0],
            };

            let packet = raptorq::EncodingPacket::deserialize(first_datagram);
            cur_id = packet.payload_id().source_block_number();

            let mut id = cur_id;
            let last = id.wrapping_add(WINDOW_WIDTH);
            while id != last {
                blocks_ignore[usize::from(id)] = false;
                id = id.wrapping_add(1);
            }
        }

        match datagrams {
            udp::Datagrams::Single(datagram) => {
                let packet = raptorq::EncodingPacket::deserialize(&datagram);
                let id = usize::from(packet.payload_id().source_block_number());
                if !blocks_ignore[id] {
                    blocks_data[id].push(packet);
                }
            }
            udp::Datagrams::Multiple(datagrams) => {
                datagrams
                    .into_iter()
                    .map(|datagram| {
                        let packet = raptorq::EncodingPacket::deserialize(&datagram);
                        let id = usize::from(packet.payload_id().source_block_number());
                        (id, packet)
                    })
                    .filter(|(id, _)| !blocks_ignore[*id])
                    .for_each(|(id, packet)| blocks_data[id].push(packet));
            }
        }

        while blocks_data[usize::from(cur_id)].len() >= min_nb_packets {
            let packets = mem::replace(
                &mut blocks_data[usize::from(cur_id)],
                Vec::with_capacity(nb_packets),
            );

            log::trace!("reassembled block {cur_id}");

            receiver.to_decode.send(super::Reassembled::Block {
                id: cur_id,
                packets,
            })?;

            blocks_ignore[usize::from(cur_id)] = true;

            let opposite = usize::from(cur_id.wrapping_add(WINDOW_WIDTH));
            blocks_ignore[opposite] = false;

            if !blocks_data[opposite].is_empty() {
                log::error!("lost block {opposite} (too far)");
                receiver.to_decode.send(super::Reassembled::Error)?;
                reset = true;
                break;
            }

            cur_id = cur_id.wrapping_add(1);
        }

        thread::yield_now();
    }
}
