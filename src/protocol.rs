//! Definition of the Lidi protocol used to transfer data over UDP
//!
//! The Lidi protocol is rather simple: since the communications are unidirectional, it is defined
//! by the blocks structure. There are 5 block types:
//! - `BlockType::Heartbeat` lets know the receiver that transfer can happen,
//! - `BlockType::Start` informs the receiver that the sent data chunk represents the beginning of
//!   a new transfer,
//! - `BlockType::Data` is used to send a data chunk that is not the beginning nor the ending of
//!   a transfer,
//! - `BlockType::Abort` informs the receiver that the current transfer has been aborted on the
//!   sender side,
//! - `BlockType::End` informs the receiver that the current transfer is completed (i.e. all
//!   data have been sent).
//!
//! A block is stored in a `Vec` of `u8`s, with the following representation:
//!
//! ```text
//!
//! <-- 4 bytes -> <-- 1 byte --> <-- 4 bytes -->
//! --------------+--------------+---------------+--------------------------------------
//! |             |              |               |                                     |
//! |  client_id  |  block_type  |  data_length  |  payload = data + optional padding  |
//! |             |              |               |                                     |
//! --------------+--------------+---------------+--------------------------------------
//!  <----------- SERIALIZE_OVERHEAD -----------> <----------- block_length ----------->
//!
//! ```
//!
//! 4-bytes values are encoded in little-endian byte order.
//!
//! In `Heartbeat` blocks, `client_id` is unused and should be set to 0 by the constructor
//! caller. Also no data payload should be provided by the constructor caller in case the block
//! is of type `Heartbeat`, `Abort` or `End`. Then the `data_length` will be set to 0 by the
//! block constructor and the data chunk will be fully padded with zeros.

use std::{fmt, io, num, sync};

pub enum Error {
    Io(io::Error),
    InvalidBlockType(Option<u8>),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::InvalidBlockType(b) => write!(fmt, "invalid block type: {b:?}"),
            Self::Other(e) => write!(fmt, "{e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<num::TryFromIntError> for Error {
    fn from(e: num::TryFromIntError) -> Self {
        Self::Other(e.to_string())
    }
}

const PACKET_HEADER_SIZE: u16 = 20 + 8;
const RAPTORQ_ALIGNMENT: u16 = 8;
const RAPTORQ_HEADER_SIZE: u16 = 4;

pub struct RaptorQ {
    max_packet_size: u16,
    symbol_count: u16,
    transfer_length: u32,
    plan: raptorq::SourceBlockEncodingPlan,
    config: raptorq::ObjectTransmissionInformation,
    nb_repair_packets: u16,
}

impl RaptorQ {
    pub fn new(mtu: u16, block_size: u32, repair_percentage: u32) -> Result<Self, Error> {
        let mut max_packet_size = mtu - PACKET_HEADER_SIZE - RAPTORQ_HEADER_SIZE;
        max_packet_size -= max_packet_size % RAPTORQ_ALIGNMENT;

        let symbol_count = u16::try_from(block_size / u32::from(max_packet_size))
            .map_err(|e| Error::Other(format!("symbol_count: {e}")))?;

        let transfer_length = u32::from(max_packet_size) * u32::from(symbol_count);

        log::debug!("generating source encoding plan...");
        let plan = raptorq::SourceBlockEncodingPlan::generate(symbol_count);
        log::debug!("source encoding plan generated");

        let config = raptorq::ObjectTransmissionInformation::with_defaults(
            u64::from(transfer_length),
            max_packet_size,
        );

        let nb_repair_packets = u16::try_from(
            ((transfer_length / 100) * repair_percentage) / u32::from(max_packet_size),
        )
        .map_err(|e| Error::Other(format!("nb_repair_packets: {e}")))?;

        Ok(Self {
            max_packet_size,
            symbol_count,
            transfer_length,
            plan,
            config,
            nb_repair_packets,
        })
    }

    pub const fn block_size(&self) -> u32 {
        self.transfer_length
    }

    pub const fn min_nb_packets(&self) -> u16 {
        self.symbol_count
    }

    pub fn nb_packets(&self) -> u32 {
        u32::from(self.symbol_count) + u32::from(self.nb_repair_packets)
    }

    pub fn encode(&self, block_id: u8, data: &[u8]) -> Vec<raptorq::EncodingPacket> {
        let encoder = raptorq::SourceBlockEncoder::with_encoding_plan(
            block_id,
            &self.config,
            data,
            &self.plan,
        );
        let mut packets = encoder.source_packets();
        if 0 < self.nb_repair_packets {
            packets.extend(encoder.repair_packets(
                u32::from(self.config.symbol_size()),
                u32::from(self.nb_repair_packets),
            ));
        }
        packets
    }

    pub fn decode(&self, block_id: u8, packets: Vec<raptorq::EncodingPacket>) -> Option<Vec<u8>> {
        let mut decoder = raptorq::SourceBlockDecoder::new(
            block_id,
            &self.config,
            u64::from(self.transfer_length),
        );
        decoder.decode(packets)
    }
}

impl fmt::Display for RaptorQ {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            fmt,
            "RaptorQ max_packet_size == {} transfer_length = {} symbol_count|nb_packets == {} nb_repair_packets == {}",
            self.max_packet_size, self.transfer_length, self.symbol_count, self.nb_repair_packets
        )
    }
}

pub(crate) enum BlockType {
    Heartbeat,
    Start,
    Data,
    Abort,
    End,
}

impl BlockType {
    fn serialized(self) -> u8 {
        match self {
            Self::Heartbeat => ID_HEARTBEAT,
            Self::Start => ID_START,
            Self::Data => ID_DATA,
            Self::Abort => ID_ABORT,
            Self::End => ID_END,
        }
    }
}

impl fmt::Display for BlockType {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Heartbeat => write!(fmt, "Heartbeat"),
            Self::Start => write!(fmt, "Start"),
            Self::Data => write!(fmt, "Data"),
            Self::Abort => write!(fmt, "Abort"),
            Self::End => write!(fmt, "End"),
        }
    }
}

const ID_HEARTBEAT: u8 = 0x00;
const ID_START: u8 = 0x01;
const ID_DATA: u8 = 0x02;
const ID_ABORT: u8 = 0x03;
const ID_END: u8 = 0x04;

pub type ClientId = u32;

static CLIENT_ID_COUNTER: sync::atomic::AtomicU32 = sync::atomic::AtomicU32::new(0);

pub(crate) fn new_client_id() -> ClientId {
    CLIENT_ID_COUNTER.fetch_add(1, sync::atomic::Ordering::Relaxed)
}

pub(crate) struct Block(Vec<u8>);

const SERIALIZE_OVERHEAD: usize = 4 + 1 + 4;

impl Block {
    /// Block constructor, craft a block according to the representation introduced in
    /// [`crate::protocol`].
    ///
    /// Some (unchecked) constraints on arguments must be respected:
    /// - if `block` is `BlockType::Heartbeat`, `BlockType::Abort` or `BlockType::End`
    ///   then no data should be provided,
    /// - if `block` is `BlockType::Heartbeat` then `client_id` should be equal to 0,
    /// - if there is some `data`, its length must be lower than `Messsage::max_data_len()`.
    pub(crate) fn new(
        block: BlockType,
        raptorq: &RaptorQ,
        client_id: ClientId,
        data: Option<&[u8]>,
    ) -> Result<Self, Error> {
        match data {
            None => {
                let mut content = vec![
                    0u8;
                    usize::try_from(raptorq.transfer_length).map_err(|e| {
                        Error::Other(format!("transfer_length: {e}"))
                    })?
                ];
                let bytes = client_id.to_le_bytes();
                content[0] = bytes[0];
                content[1] = bytes[1];
                content[2] = bytes[2];
                content[3] = bytes[3];
                content[4] = block.serialized();
                Ok(Self(content))
            }
            Some(data) => {
                let mut content = Vec::with_capacity(
                    usize::try_from(raptorq.transfer_length)
                        .map_err(|e| Error::Other(format!("transfer_length: {e}")))?,
                );
                content.extend_from_slice(&client_id.to_le_bytes());
                content.push(block.serialized());
                content.extend_from_slice(&u32::to_le_bytes(
                    u32::try_from(data.len())
                        .map_err(|e| Error::Other(format!("data.len(): {e}")))?,
                ));
                content.extend_from_slice(data);
                if content.len() < content.capacity() {
                    content.resize(content.capacity(), 0);
                }
                Ok(Self(content))
            }
        }
    }

    pub(crate) fn client_id(&self) -> ClientId {
        let bytes = [self.0[0], self.0[1], self.0[2], self.0[3]];
        u32::from_le_bytes(bytes)
    }

    pub(crate) fn block_type(&self) -> Result<BlockType, Error> {
        match self.0.get(4) {
            Some(&ID_HEARTBEAT) => Ok(BlockType::Heartbeat),
            Some(&ID_START) => Ok(BlockType::Start),
            Some(&ID_DATA) => Ok(BlockType::Data),
            Some(&ID_ABORT) => Ok(BlockType::Abort),
            Some(&ID_END) => Ok(BlockType::End),
            b => Err(Error::InvalidBlockType(b.copied())),
        }
    }

    fn payload_len(&self) -> u32 {
        let data_len_bytes = [self.0[5], self.0[6], self.0[7], self.0[8]];
        u32::from_le_bytes(data_len_bytes)
    }

    pub(crate) const fn deserialize(data: Vec<u8>) -> Self {
        Self(data)
    }

    pub fn max_data_len(raptorq: &RaptorQ) -> usize {
        raptorq.transfer_length as usize - SERIALIZE_OVERHEAD
    }

    pub(crate) fn payload(&self) -> &[u8] {
        let len = self.payload_len();
        &self.0[SERIALIZE_OVERHEAD..(SERIALIZE_OVERHEAD + len as usize)]
    }

    pub(crate) fn serialized(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for Block {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let msg_type = match self.block_type() {
            Err(e) => format!("UNKNOWN {e}"),
            Ok(t) => t.to_string(),
        };
        write!(
            fmt,
            "client {:x} block = {} data = {} byte(s)",
            self.client_id(),
            msg_type,
            self.payload_len()
        )
    }
}
