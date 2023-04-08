use crate::protocol;
use crossbeam_channel::{self, Receiver, RecvError, SendError, Sender};
use log::{debug, error, trace, warn};
use raptorq::{
    EncodingPacket, ObjectTransmissionInformation, SourceBlockEncoder, SourceBlockEncodingPlan,
};
use std::{fmt, sync::Mutex};

#[derive(Clone)]
pub struct Config {
    pub object_transmission_info: ObjectTransmissionInformation,
    pub repair_block_size: u32,
}

enum Error {
    Receive(RecvError),
    Send(SendError<Vec<EncodingPacket>>),
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

impl From<RecvError> for Error {
    fn from(e: RecvError) -> Self {
        Self::Receive(e)
    }
}

impl From<SendError<Vec<EncodingPacket>>> for Error {
    fn from(e: SendError<Vec<EncodingPacket>>) -> Self {
        Self::Send(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Diode(e)
    }
}

pub fn new(
    config: &Config,
    block_to_encode: &Mutex<u8>,
    block_to_send: &Mutex<u8>,
    recvq: &Receiver<protocol::Message>,
    sendq: &Sender<Vec<EncodingPacket>>,
) {
    if let Err(e) = main_loop(config, block_to_encode, block_to_send, recvq, sendq) {
        error!("encoding loop error: {e}");
    }
}

fn main_loop(
    config: &Config,
    block_to_encode: &Mutex<u8>,
    block_to_send: &Mutex<u8>,
    recvq: &Receiver<protocol::Message>,
    sendq: &Sender<Vec<EncodingPacket>>,
) -> Result<(), Error> {
    let nb_repair_packets =
        protocol::nb_repair_packets(&config.object_transmission_info, config.repair_block_size);

    if nb_repair_packets == 0 {
        warn!("configuration produces 0 repair packet");
    }

    let sbep = SourceBlockEncodingPlan::generate(
        (config.object_transmission_info.transfer_length()
            / config.object_transmission_info.symbol_size() as u64) as u16,
    );

    loop {
        let mut block_id_to_encode = block_to_encode.lock().expect("acquire lock");
        let message = recvq.recv()?;
        let block_id = *block_id_to_encode;
        *block_id_to_encode = block_id_to_encode.wrapping_add(1);
        drop(block_id_to_encode);

        let message_type = message.message_type()?;
        let client_id = message.client_id();

        match message_type {
            protocol::MessageType::Start => debug!(
                "start of encoding block {block_id} for client {:x}",
                client_id
            ),
            protocol::MessageType::End => debug!(
                "end of encoding block {block_id} for client {:x}",
                client_id
            ),
            _ => (),
        }

        let data = message.serialized();

        trace!("encoding a serialized block of {} bytes", data.len());

        let encoder = SourceBlockEncoder::with_encoding_plan2(
            block_id,
            &config.object_transmission_info,
            data,
            &sbep,
        );

        let mut packets = encoder.source_packets();

        if 0 < nb_repair_packets {
            packets.extend(encoder.repair_packets(0, nb_repair_packets));
        }

        loop {
            let mut to_send = block_to_send.lock().expect("acquire lock");
            if *to_send == block_id {
                sendq.send(packets)?;
                *to_send = to_send.wrapping_add(1);
                break;
            }
        }
    }
}
