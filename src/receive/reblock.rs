use crate::protocol;
use crossbeam_channel::{Receiver, RecvTimeoutError, SendError, Sender};
use log::{debug, error, info, trace, warn};
use raptorq::{self, EncodingPacket, ObjectTransmissionInformation};
use std::{fmt, sync::Mutex, time::Duration};

pub struct Config {
    pub object_transmission_info: ObjectTransmissionInformation,
    pub repair_block_size: u32,
    pub flush_timeout: Duration,
}

enum Error {
    Receive(RecvTimeoutError),
    Crossbeam(SendError<(u8, Vec<EncodingPacket>)>),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::Crossbeam(e) => write!(fmt, "crossbeam error: {e}"),
        }
    }
}

impl From<RecvTimeoutError> for Error {
    fn from(e: RecvTimeoutError) -> Self {
        Self::Receive(e)
    }
}

impl From<SendError<(u8, Vec<EncodingPacket>)>> for Error {
    fn from(e: SendError<(u8, Vec<EncodingPacket>)>) -> Self {
        Self::Crossbeam(e)
    }
}

pub fn new(
    config: &Config,
    block_to_receive: &Mutex<u8>,
    udp_recvq: &Receiver<Vec<EncodingPacket>>,
    decoding_sendq: &Sender<(u8, Vec<EncodingPacket>)>,
) {
    if let Err(e) = main_loop(config, block_to_receive, udp_recvq, decoding_sendq) {
        error!("reblock loop error: {e}");
    }
}

fn main_loop(
    config: &Config,
    block_to_receive: &Mutex<u8>,
    udp_recvq: &Receiver<Vec<EncodingPacket>>,
    decoding_sendq: &Sender<(u8, Vec<EncodingPacket>)>,
) -> Result<(), Error> {
    let encoding_block_size = config.object_transmission_info.transfer_length();

    let nb_normal_packets = protocol::nb_encoding_packets(&config.object_transmission_info);
    let nb_repair_packets =
        protocol::nb_repair_packets(&config.object_transmission_info, config.repair_block_size);

    info!(
        "reblock will expect at least {} packets ({} bytes per block) + flush timeout of {} ms",
        nb_normal_packets,
        encoding_block_size,
        config.flush_timeout.as_millis(),
    );

    let mut desynchro = true;
    let capacity = nb_normal_packets as usize + nb_repair_packets as usize;
    let mut prev_queue: Option<Vec<EncodingPacket>> = None;
    let mut queue = Vec::with_capacity(capacity);
    let mut block_id = 0;

    loop {
        let packets = match udp_recvq.recv_timeout(config.flush_timeout) {
            Err(RecvTimeoutError::Timeout) => {
                let qlen = queue.len();
                if 0 < qlen {
                    // no more traffic but ongoing block, trying to decode
                    if nb_normal_packets as usize <= qlen {
                        debug!("flushing block {block_id} with {qlen} packets");
                        decoding_sendq.send((block_id, queue))?;
                        block_id = block_id.wrapping_add(1);
                    } else {
                        debug!("no enough packets ({qlen} packates) to decode block {block_id}");
                        warn!("lost block {block_id}");
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
            Err(e) => return Err(Error::from(e)),
            Ok(packet) => packet,
        };

        for packet in packets.into_iter() {
            let payload_id = packet.payload_id();
            let message_block_id = payload_id.source_block_number();

            if desynchro {
                block_id = message_block_id;
                *block_to_receive.lock().expect("acquire lock") = block_id;
                desynchro = false;
            }

            if message_block_id == block_id {
                trace!("queueing in block {block_id}");
                queue.push(packet);
                continue;
            }

            if message_block_id.wrapping_add(1) == block_id {
                //packet is from previous block; is this block parked ?
                if let Some(mut pqueue) = prev_queue {
                    pqueue.push(packet);
                    if nb_normal_packets as usize <= pqueue.len() {
                        //now there is enough packets to decode it
                        decoding_sendq.send((message_block_id, pqueue))?;
                        prev_queue = None;
                    } else {
                        prev_queue = Some(pqueue);
                    }
                }
                continue;
            }

            if message_block_id != block_id.wrapping_add(1) {
                warn!("discarding packet with block_id {message_block_id} (current block_id is {block_id})");
                continue;
            }

            //this is the first packet of the next block

            if nb_normal_packets as usize <= queue.len() {
                //enough packets in the current block to decode it
                decoding_sendq.send((block_id, queue))?;
                if prev_queue.is_some() {
                    warn!("lost block {}", block_id.wrapping_sub(1));
                }
                prev_queue = None;
            } else {
                //not enough packet, parking the current block
                prev_queue = Some(queue);
            }

            //starting the next block

            block_id = message_block_id;

            trace!("queueing in block {block_id}");
            queue = Vec::with_capacity(capacity);
            queue.push(packet);
        }
    }
}
