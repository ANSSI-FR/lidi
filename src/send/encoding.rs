use crate::protocol;
use crossbeam_channel::{self, Receiver, RecvTimeoutError, SendError, Sender};
use log::{debug, error, info, trace, warn};
use raptorq::{ObjectTransmissionInformation, SourceBlockEncoder, SourceBlockEncodingPlan};
use std::{collections::VecDeque, fmt, time::Duration};

use super::devector;

pub struct Config {
    pub logical_block_size: u64,
    pub repair_block_size: u32,
    pub output_mtu: u16,
    pub flush_timeout: Duration,
}

enum Error {
    Receive(RecvTimeoutError),
    Send(SendError<devector::Message>),
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

impl From<SendError<devector::Message>> for Error {
    fn from(e: SendError<devector::Message>) -> Self {
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
    sendq: Sender<devector::Message>,
) {
    if let Err(e) = main_loop(config, recvq, sendq) {
        error!("encoding loop error: {e}");
    }
}

fn main_loop(
    config: Config,
    recvq: Receiver<protocol::ClientMessage>,
    sendq: Sender<devector::Message>,
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

    let sbep = SourceBlockEncodingPlan::generate(
        (config.logical_block_size / oti.symbol_size() as u64) as u16,
    );

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

            let encoder = SourceBlockEncoder::with_encoding_plan2(block_id, &oti, data, &sbep);

            let _ = queue.drain(0..config.logical_block_size as usize);
            trace!("after flushing queue len = {}", queue.len());

            sendq.send(encoder.source_packets())?;

            if 0 < nb_repair_packets {
                sendq.send(encoder.repair_packets(0, nb_repair_packets))?;
            }

            block_id = block_id.wrapping_add(1);
        }
    }
}
