//! Worker for grouping packets according to their block numbers to handle potential UDP packets
//! reordering

use crate::{ClientLifecycle, dispatch};
use lidi_protocol as protocol;
use std::{array, mem};

pub const WINDOW_WIDTH: u8 = u8::MAX / 2;

struct Block {
    ignore: bool,
    packets: Vec<raptorq::EncodingPacket>,
}

fn send_to_dispatch<Lifecycle>(
    receiver: &crate::Receiver<Lifecycle>,
    session_id: protocol::SessionId,
    id: u8,
    blocks: &mut [Block],
    packet_vec_pool: &mut Vec<Vec<raptorq::EncodingPacket>>,
) -> Result<bool, crate::Error>
where
    Lifecycle: ClientLifecycle,
{
    blocks[id as usize].ignore = true;

    let capacity = blocks[id as usize].packets.capacity();
    let replacement = packet_vec_pool
        .pop()
        .unwrap_or_else(|| Vec::with_capacity(capacity));
    let mut packets = mem::replace(&mut blocks[id as usize].packets, replacement);

    let nb_packets = packets.len();

    log::debug!("received block {id} to decode ({nb_packets} packets)");

    #[cfg(feature = "prometheus")]
    #[allow(clippy::cast_precision_loss)]
    metrics::histogram!("lidi_receive_decode_with_n_packets").record(packets.len() as f64);

    // `drain` empties `packets` into the decoder without giving up its allocation, so it can be
    // pushed back onto the pool below for the next block to reuse. `into_iter()` (clippy's usual
    // suggestion) would drop the allocation instead, defeating the point of the pool.
    #[allow(clippy::iter_with_drain)]
    let decoded = receiver.raptorq.decode(id, packets.drain(..));
    packet_vec_pool.push(packets);

    match decoded {
        None => {
            #[cfg(feature = "prometheus")]
            metrics::counter!("lidi_receive_blocks_decode_failed").increment(1);

            log::error!("lost block {id} (failed to decode with {nb_packets} packets)");

            receiver.to_dispatch.send(dispatch::Message::LostBlock)?;
        }
        Some(block) => {
            #[cfg(feature = "prometheus")]
            metrics::counter!("lidi_receive_blocks_decoded").increment(1);

            log::trace!("block {id} decoded ({} bytes)", block.len());

            receiver.to_dispatch.send(dispatch::Message::Block(
                session_id,
                protocol::Block::deserialize(block),
            ))?;
        }
    }

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
            log::warn!("synchronization lost received, propagating");
            receiver.to_dispatch.send(dispatch::Message::LostBlock)?;
            return Ok(true);
        }
    }

    Ok(false)
}

// Forcibly decodes every non-empty block still in the current window: called on a reset
// timeout, since no more packets are coming for this window.
fn flush_pending_blocks<Lifecycle>(
    receiver: &crate::Receiver<Lifecycle>,
    session_id: protocol::SessionId,
    cur_id: &mut u8,
    min_nb_packets: usize,
    blocks: &mut [Block],
    packet_vec_pool: &mut Vec<Vec<raptorq::EncodingPacket>>,
) -> Result<(), crate::Error>
where
    Lifecycle: ClientLifecycle,
{
    let prev = cur_id.wrapping_sub(1);
    while *cur_id != prev {
        let nb_packets = blocks[*cur_id as usize].packets.len();
        if 0 < nb_packets {
            if nb_packets < min_nb_packets {
                log::warn!(
                    "block {cur_id} is incomplete ({nb_packets} packets) after reset timeout, forcibly send to decode"
                );
                #[cfg(feature = "prometheus")]
                metrics::counter!("lidi_receive_blocks_lost").increment(1);
            }
            let _ = send_to_dispatch(receiver, session_id, *cur_id, blocks, packet_vec_pool)?;
        }
        *cur_id = cur_id.wrapping_add(1);
    }
    Ok(())
}

// Resets the block window to start at `first_packet`'s block id, called after a reset timeout
// or a new session. Returns the new `cur_id`.
fn start_new_window(blocks: &mut [Block], first_packet: &raptorq::EncodingPacket) -> u8 {
    for block in &mut *blocks {
        block.ignore = true;
        block.packets.clear();
    }

    let cur_id = first_packet.payload_id().source_block_number();

    let mut id = cur_id;
    let last = id.wrapping_add(WINDOW_WIDTH);
    while id != last {
        blocks[id as usize].ignore = false;
        id = id.wrapping_add(1);
    }

    cur_id
}

pub enum Message {
    NewSession(protocol::SessionId),
    #[cfg(not(feature = "receive-mmsg"))]
    Packet(raptorq::EncodingPacket),
    #[cfg(feature = "receive-mmsg")]
    Packets(Vec<raptorq::EncodingPacket>),
}

pub fn start<Lifecycle>(
    receiver: &crate::Receiver<Lifecycle>,
    for_reblock: &crossbeam_channel::Receiver<Message>,
) -> Result<(), crate::Error>
where
    Lifecycle: ClientLifecycle,
{
    let min_nb_packets = usize::try_from(receiver.raptorq.min_nb_packets())
        .map_err(|e| crate::Error::Internal(format!("min_nb_packets: {e}")))?;
    let nb_packets = usize::try_from(receiver.raptorq.nb_packets())
        .map_err(|e| crate::Error::Internal(format!("nb_packets: {e}")))?;

    let mut blocks: [_; u8::MAX as usize + 1] = array::from_fn(|_| Block {
        ignore: true,
        packets: Vec::with_capacity(nb_packets),
    });

    let mut session_id = 0;

    let mut cur_id: u8 = 0;

    let mut reset = true;

    // Recycled across blocks: `send_to_dispatch` pops a drained `Vec` from here instead of
    // allocating, and pushes the one it just drained back once decoding is done.
    let mut packet_vec_pool: Vec<Vec<raptorq::EncodingPacket>> = Vec::new();

    loop {
        // Only mutated (via `drain`) when receive-mmsg is enabled, to reclaim the Vec below.
        #[cfg_attr(not(feature = "receive-mmsg"), allow(unused_mut))]
        let mut packets = match for_reblock.recv_timeout(receiver.config.reset_timeout) {
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if !reset {
                    log::debug!("reset timeout reached, flushing");

                    reset = true;

                    flush_pending_blocks(
                        receiver,
                        session_id,
                        &mut cur_id,
                        min_nb_packets,
                        &mut blocks,
                        &mut packet_vec_pool,
                    )?;
                }

                continue;
            }
            Err(e) => return Err(crate::Error::from(e)),
            Ok(message) => match message {
                Message::NewSession(new_session_id) => {
                    reset = true;

                    session_id = new_session_id;

                    log::debug!("new session is {session_id:x}");

                    receiver
                        .to_dispatch
                        .send(dispatch::Message::NewSession(session_id))?;

                    continue;
                }
                #[cfg(not(feature = "receive-mmsg"))]
                Message::Packet(packet) => [packet],
                #[cfg(feature = "receive-mmsg")]
                Message::Packets(packets) => packets,
            },
        };

        if reset {
            reset = false;
            cur_id = start_new_window(&mut blocks, &packets[0]);
        }

        let mut fast_track = false;

        let block_id_for_fast_track = cur_id.wrapping_add(WINDOW_WIDTH);

        let mut distribute = |packet: raptorq::EncodingPacket| {
            let id = packet.payload_id().source_block_number();

            if id == block_id_for_fast_track {
                fast_track = true;
                blocks[id as usize].ignore = false;
            }

            if blocks[id as usize].ignore {
                #[cfg(feature = "prometheus")]
                metrics::counter!("lidi_receive_packets_ignored").increment(1);
            } else {
                blocks[id as usize].packets.push(packet);
            }
        };

        // Non-mmsg: `packets` is a stack array, consumed by value. Mmsg: `packets` is a `Vec`,
        // drained rather than consumed so the emptied allocation can be sent back to
        // `packet_vec_recycler` below for the udp worker to reuse. Either way `packet_iter`
        // yields owned `EncodingPacket`s, so the distribution loop itself is written once.
        #[cfg(not(feature = "receive-mmsg"))]
        let packet_iter = packets.into_iter();
        #[cfg(feature = "receive-mmsg")]
        #[allow(clippy::iter_with_drain)]
        let packet_iter = packets.drain(..);

        for packet in packet_iter {
            distribute(packet);
        }

        #[cfg(feature = "receive-mmsg")]
        receiver.packet_vec_recycler.push(packets);

        if fast_track {
            log::warn!("probable network interrupt, fast track first block");
            let _ = send_to_dispatch(
                receiver,
                session_id,
                cur_id,
                &mut blocks,
                &mut packet_vec_pool,
            )?;
            cur_id = cur_id.wrapping_add(1);
        }

        while blocks[cur_id as usize].packets.len() >= min_nb_packets {
            reset = send_to_dispatch(
                receiver,
                session_id,
                cur_id,
                &mut blocks,
                &mut packet_vec_pool,
            )?;
            cur_id = cur_id.wrapping_add(1);
        }
    }
}
