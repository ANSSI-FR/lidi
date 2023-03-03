use crate::protocol;
use crossbeam_channel::{Receiver, RecvError, SendError, Sender};
use log::{error, trace, warn};
use raptorq::{self, EncodingPacket, ObjectTransmissionInformation, SourceBlockDecoder};
use std::{fmt, sync::Mutex, thread};

pub struct Config {
    pub object_transmission_info: ObjectTransmissionInformation,
}

enum Error {
    Receive(RecvError),
    Crossbeam(SendError<protocol::Message>),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::Crossbeam(e) => write!(fmt, "crossbeam error: {e}"),
        }
    }
}

impl From<RecvError> for Error {
    fn from(e: RecvError) -> Self {
        Self::Receive(e)
    }
}

impl From<SendError<protocol::Message>> for Error {
    fn from(e: SendError<protocol::Message>) -> Self {
        Self::Crossbeam(e)
    }
}

pub fn new(
    config: &Config,
    block_to_receive: &Mutex<u8>,
    reblock_recvq: &Receiver<(u8, Vec<EncodingPacket>)>,
    dispatch_sendq: &Sender<protocol::Message>,
) {
    if let Err(e) = main_loop(config, block_to_receive, reblock_recvq, dispatch_sendq) {
        error!("decoding loop error: {e}");
    }
}

fn main_loop(
    config: &Config,
    block_to_receive: &Mutex<u8>,
    reblock_recvq: &Receiver<(u8, Vec<EncodingPacket>)>,
    dispatch_sendq: &Sender<protocol::Message>,
) -> Result<(), Error> {
    let encoding_block_size = config.object_transmission_info.transfer_length();

    loop {
        let (block_id, packets) = reblock_recvq.recv()?;

        trace!(
            "trying to decode block {block_id} with {} packets",
            packets.len()
        );

        let mut decoder = SourceBlockDecoder::new2(
            block_id,
            &config.object_transmission_info,
            encoding_block_size,
        );

        match decoder.decode(packets) {
            None => {
                warn!("lost block {block_id}");
                continue;
            }
            Some(block) => {
                trace!("block {} decoded with {} bytes!", block_id, block.len());

                loop {
                    let mut to_receive = block_to_receive.lock().expect("acquire lock");
                    if *to_receive == block_id {
                        dispatch_sendq.send(protocol::Message::deserialize(block))?;
                        *to_receive = to_receive.wrapping_add(1);
                        break;
                    }
                }
            }
        }
    }
}
