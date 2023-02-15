use crate::protocol;
use crossbeam_channel::{self, Receiver, RecvError, SendError, Sender};
use log::{debug, error, info, trace, warn};
use raptorq::{
    EncodingPacket, ObjectTransmissionInformation, SourceBlockEncoder, SourceBlockEncodingPlan,
};
use std::fmt;

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

pub fn new(config: Config, recvq: Receiver<protocol::Message>, sendq: Sender<Vec<EncodingPacket>>) {
    if let Err(e) = main_loop(config, recvq, sendq) {
        error!("encoding loop error: {e}");
    }
}

fn main_loop(
    config: Config,
    recvq: Receiver<protocol::Message>,
    sendq: Sender<Vec<EncodingPacket>>,
) -> Result<(), Error> {
    let nb_repair_packets =
        config.repair_block_size / protocol::data_mtu(&config.object_transmission_info) as u32;

    let encoding_block_size = config.object_transmission_info.transfer_length() as usize;

    info!(
        "encoding will produce {} packets ({} bytes per block) + {} repair packets",
        protocol::nb_encoding_packets(&config.object_transmission_info),
        encoding_block_size,
        nb_repair_packets
    );

    if nb_repair_packets == 0 {
        warn!("configuration produces 0 repair packet");
    }

    let sbep = SourceBlockEncodingPlan::generate(
        (config.object_transmission_info.transfer_length()
            / config.object_transmission_info.symbol_size() as u64) as u16,
    );

    let mut block_id = 0;

    loop {
        let message = recvq.recv()?;

        let message_type = message.message_type()?;
        let client_id = message.client_id();

        match message_type {
            protocol::MessageType::Start => debug!("start of encoding of client {:x}", client_id),
            protocol::MessageType::End => debug!("end of encoding of client {:x}", client_id),
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

        sendq.send(packets)?;

        block_id = block_id.wrapping_add(1);
    }
}
