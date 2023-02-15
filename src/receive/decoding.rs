use crate::{protocol, receive::dispatch};
use crossbeam_channel::{Receiver, RecvTimeoutError, SendError, Sender};
use log::{debug, error, info, trace, warn};
use raptorq::{self, EncodingPacket, ObjectTransmissionInformation, SourceBlockDecoder};
use std::{fmt, time::Duration};

pub struct Config {
    pub object_transmission_info: ObjectTransmissionInformation,
    pub flush_timeout: Duration,
}

enum Error {
    Receive(RecvTimeoutError),
    Crossbeam(SendError<dispatch::Message>),
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

impl From<SendError<dispatch::Message>> for Error {
    fn from(e: SendError<dispatch::Message>) -> Self {
        Self::Crossbeam(e)
    }
}

pub type Message = EncodingPacket;

pub fn new(
    config: Config,
    udp_recvq: Receiver<Message>,
    dispatch_sendq: Sender<dispatch::Message>,
) {
    if let Err(e) = main_loop(config, udp_recvq, dispatch_sendq) {
        error!("decoding loop error: {e}");
    }
}

fn main_loop(
    config: Config,
    udp_recvq: Receiver<Message>,
    dispatch_sendq: Sender<dispatch::Message>,
) -> Result<(), Error> {
    let encoding_block_size = config.object_transmission_info.transfer_length();

    let nb_normal_packets = config.object_transmission_info.transfer_length()
        / config.object_transmission_info.symbol_size() as u64;

    info!(
        "decoding will expect {} packets ({} bytes per block) + flush timeout of {} ms",
        protocol::nb_encoding_packets(&config.object_transmission_info),
        encoding_block_size,
        config.flush_timeout.as_millis()
    );

    let mut desynchro = true;
    let mut queue = Vec::with_capacity(nb_normal_packets as usize);
    let mut block_id = 0;

    loop {
        let packet = match udp_recvq.recv_timeout(config.flush_timeout) {
            Err(RecvTimeoutError::Timeout) => {
                let qlen = queue.len();
                if 0 < qlen {
                    // no more traffic but ongoing block, trying to decode
                    debug!("flush timeout with {qlen} packets");

                    if nb_normal_packets as usize <= qlen {
                        debug!("trying to decode");
                        let mut decoder = SourceBlockDecoder::new2(
                            block_id,
                            &config.object_transmission_info,
                            encoding_block_size,
                        );

                        match decoder.decode(queue) {
                            None => {
                                warn!("lost block {block_id}");
                                desynchro = true;
                            }
                            Some(block) => {
                                trace!("block {} received with {} bytes!", block_id, block.len());
                                dispatch_sendq.send(protocol::ClientMessage::deserialize(block))?;
                                block_id = block_id.wrapping_add(1);
                            }
                        };
                    } else {
                        debug!("no enough packets to decode, discarding");
                        warn!("lost block {block_id}");
                        desynchro = true;
                    }
                    queue = Vec::with_capacity(nb_normal_packets as usize);
                } else {
                    // without data for some time we reset the current block_id
                    desynchro = true;
                }
                continue;
            }
            Err(e) => return Err(Error::from(e)),
            Ok(packet) => packet,
        };

        let payload_id = packet.payload_id();
        let message_block_id = payload_id.source_block_number();

        if desynchro {
            block_id = message_block_id;
            desynchro = false;
        }

        if message_block_id == block_id {
            trace!("queueing in block {block_id}");
            queue.push(packet);
            continue;
        }

        if message_block_id.wrapping_add(1) == block_id {
            trace!("discarding packet from previous block_id {message_block_id}");
            continue;
        }

        if message_block_id != block_id.wrapping_add(1) {
            warn!("discarding packet with block_id {message_block_id} (current block_id is {block_id})");
            continue;
        }

        // message block_id is from next block, flushing current block
        let mut decoder = SourceBlockDecoder::new2(
            block_id,
            &config.object_transmission_info,
            encoding_block_size,
        );

        match decoder.decode(queue) {
            None => warn!("lost block {block_id}"),
            Some(block) => {
                trace!("block {} received with {} bytes!", block_id, block.len());
                dispatch_sendq.send(protocol::ClientMessage::deserialize(block))?;
            }
        }

        block_id = message_block_id;
        trace!("queueing in block {block_id}");
        queue = Vec::with_capacity(nb_normal_packets as usize);
        queue.push(packet);
    }
}
