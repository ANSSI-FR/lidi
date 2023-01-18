use crate::protocol;
use crate::send::udp_send;
use crossbeam_channel::{self, Receiver, RecvTimeoutError, SendError, Sender};
use log::{debug, error, info, trace, warn};
use raptorq::{ObjectTransmissionInformation, SourceBlockEncoder};
use std::{collections::VecDeque, fmt, time::Duration};

#[derive(Clone)]
pub struct Config {
    pub logical_block_size: u64,
    pub repair_block_size: u32,
    pub output_mtu: u16,
    pub flush_timeout: Duration,
}

enum Error {
    Receive(RecvTimeoutError),
    Send(SendError<udp_send::Message>),
    Diode(protocol::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Receive(e) => write!(fmt, "crossbeam recv error: {e}"),
            Self::Send(e) => write!(fmt, "crossbeam send error: {e}"),
            Self::Diode(e) => write!(fmt, "diode error: {e}"),
        }
    }
}

impl From<RecvTimeoutError> for Error {
    fn from(e: RecvTimeoutError) -> Self {
        Self::Receive(e)
    }
}

impl From<SendError<udp_send::Message>> for Error {
    fn from(e: SendError<udp_send::Message>) -> Self {
        Self::Send(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Diode(e)
    }
}

pub fn new(
    config: Config,
    recvq: Receiver<protocol::ClientMessage>,
    sendq: Sender<udp_send::Message>,
) {
    if let Err(e) = main_loop(config, recvq, sendq) {
        error!("encoding loop error: {e}");
    }
}

fn main_loop(
    config: Config,
    recvq: Receiver<protocol::ClientMessage>,
    sendq: Sender<udp_send::Message>,
) -> Result<(), Error> {
    let nb_repair_packets = config.repair_block_size / config.output_mtu as u32;

    if nb_repair_packets == 0 {
        warn!("configuration produces 0 repair packets");
    } else {
        info!(
            "{nb_repair_packets} repair packets ({} bytes) per encoding block will be produced",
            nb_repair_packets * config.output_mtu as u32
        );
    }

    let oti =
        ObjectTransmissionInformation::with_defaults(config.logical_block_size, config.output_mtu);

    debug!("object transformation information = {:?} ", oti);

    let overhead = protocol::ClientMessage::serialize_padding_overhead();

    debug!("padding encoding overhead is {} bytes", overhead);

    let mut queue = VecDeque::with_capacity(config.logical_block_size as usize);

    let mut block_id = 0;

    loop {
        let message = match recvq.recv_timeout(config.flush_timeout) {
            Err(RecvTimeoutError::Timeout) => {
                trace!("flush timeout");
                if queue.is_empty() {
                    continue;
                }
                let padding_needed = config.logical_block_size as usize - queue.len();
                let padding_len = if padding_needed < overhead {
                    debug!("top much padding overhead !");
                    0
                } else {
                    padding_needed - overhead
                };
                debug!("flushing with {padding_len} padding bytes");
                protocol::ClientMessage {
                    client_id: 0,
                    payload: protocol::Message::Padding(padding_len as u32),
                }
            }
            Err(e) => return Err(Error::from(e)),
            Ok(message) => message,
        };

        message.serialize_to(&mut queue)?;

        match message.payload {
            protocol::Message::Start => {
                debug!("start of encoding of client {:x}", message.client_id)
            }
            protocol::Message::End => debug!("end of encoding of client {:x}", message.client_id),
            _ => (),
        }

        while (config.logical_block_size as usize) <= queue.len() {
            // full block, we can flush
            trace!("flushing queue len = {}", queue.len());
            let data = &queue.make_contiguous()[..config.logical_block_size as usize];

            let encoder = SourceBlockEncoder::new2(block_id, &oti, data);

            let _ = queue.drain(0..config.logical_block_size as usize);
            trace!("after flushing queue len = {}", queue.len());

            let mut total_sent = 0;
            let mut total_packets = 0;
            let mut total_repair = 0;

            for packet in encoder.source_packets() {
                total_packets += 1;
                total_sent += packet.data().len();
                sendq.send(packet)?;
            }

            if 0 < nb_repair_packets {
                for packet in encoder.repair_packets(0, nb_repair_packets) {
                    total_repair += 1;
                    total_sent += packet.data().len();
                    sendq.send(packet)?;
                }
            }

            trace!(
                "{total_sent} bytes sent, {total_packets} packets + {total_repair} repair_packets = {}", total_packets + total_repair
            );

            block_id = block_id.wrapping_add(1);
        }
    }
}
