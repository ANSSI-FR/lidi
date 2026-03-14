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

use std::{fmt, io};

pub enum Error {
    DataTooLarge(String),
    Io(io::Error),
    InvalidBlockType(Option<u8>),
    InvalidEndpoint(EndpointId),
    SymbolCountTooLarge(String),
    TransferLengthTooLarge(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::DataTooLarge(s) => write!(fmt, "data too large: {s}"),
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::InvalidBlockType(b) => write!(fmt, "invalid block type: {b:?}"),
            Self::InvalidEndpoint(e) => write!(fmt, "invalid endpoint: {e}"),
            Self::SymbolCountTooLarge(s) => write!(fmt, "symbol count too large: {s}"),
            Self::TransferLengthTooLarge(s) => write!(fmt, "transfer length too large: {s}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub const MIN_NB_REPAIR_PACKETS: u16 = 2;

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
    /// # Errors
    ///
    /// Will return `Err` if `symbol_count`
    ///   or
    /// `nb_repair_packets` parsing fails
    pub fn new(mtu: u16, block_size: u32, nb_repair_packets: u16) -> Result<Self, Error> {
        let mut max_packet_size = mtu - PACKET_HEADER_SIZE - RAPTORQ_HEADER_SIZE;
        max_packet_size -= max_packet_size % RAPTORQ_ALIGNMENT;

        let symbol_count = u16::try_from(block_size / u32::from(max_packet_size))
            .map_err(|e| Error::SymbolCountTooLarge(e.to_string()))?;

        let transfer_length = u32::from(max_packet_size) * u32::from(symbol_count);

        let plan = raptorq::SourceBlockEncodingPlan::generate(symbol_count);

        let config = raptorq::ObjectTransmissionInformation::with_defaults(
            u64::from(transfer_length),
            max_packet_size,
        );

        let mut nb_repair_packets = nb_repair_packets;
        if nb_repair_packets < MIN_NB_REPAIR_PACKETS {
            nb_repair_packets = MIN_NB_REPAIR_PACKETS;
        }

        Ok(Self {
            max_packet_size,
            symbol_count,
            transfer_length,
            plan,
            config,
            nb_repair_packets,
        })
    }

    #[must_use]
    pub const fn block_size(&self) -> u32 {
        self.transfer_length
    }

    #[must_use]
    pub const fn min_nb_packets(&self) -> u16 {
        // we require to have at least min_nb_repair_packets packets
        // in addition to normal packets to improve integrity of
        // RaptorQ decoding process
        self.symbol_count + MIN_NB_REPAIR_PACKETS
    }

    #[must_use]
    pub fn nb_packets(&self) -> u32 {
        u32::from(self.symbol_count) + u32::from(self.nb_repair_packets)
    }

    #[must_use]
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

    #[must_use]
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
            "RaptorQ max_packet_size = {} transfer_length = {} symbol_count|nb_packets = {} nb_repair_packets == {} min_nb_repair_packets == {}",
            self.max_packet_size,
            self.transfer_length,
            self.symbol_count,
            self.nb_repair_packets,
            MIN_NB_REPAIR_PACKETS
        )
    }
}

pub enum BlockType {
    Heartbeat,
    Start,
    Data,
    Abort,
    End,
}

impl BlockType {
    const fn serialized(self) -> u8 {
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

#[derive(Clone, Copy)]
pub struct EndpointId(u16);

impl EndpointId {
    #[must_use]
    pub const fn new(endpoint: u16) -> Self {
        Self(endpoint)
    }

    #[must_use]
    pub const fn value(&self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn serialize(&self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    #[must_use]
    pub const fn deserialize(payload: &[u8]) -> Option<Self> {
        if payload.len() != 2 {
            return None;
        }
        let mut endpoint = [0u8; 2];
        endpoint.copy_from_slice(payload);
        Some(Self(u16::from_le_bytes(endpoint)))
    }
}

impl fmt::Display for EndpointId {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(fmt, "{}", self.0)
    }
}

pub type ClientId = u32;

const SERIALIZE_OVERHEAD: usize = 4 + 1 + 4;

pub struct Block {
    id: u8,
    data: Vec<u8>,
}

impl Block {
    /// Block constructor, craft a block according to the representation introduced in
    /// [`crate::protocol`].
    ///
    /// Some (unchecked) constraints on arguments must be respected:
    /// - if `block` is `BlockType::Heartbeat`, `BlockType::Abort` or `BlockType::End`
    ///   then no data should be provided,
    /// - if `block` is `BlockType::Heartbeat` then `client_id` should be equal to 0,
    /// - if there is some `data`, its length must be lower than `Messsage::max_data_len()`.
    pub fn new(
        recycle: Option<Self>,
        id: u8,
        block: BlockType,
        raptorq: &RaptorQ,
        client_id: ClientId,
        data: Option<&[u8]>,
    ) -> Result<Self, Error> {
        let mut res = match recycle {
            Some(mut res) => {
                res.id = id;
                res.data[5..9].copy_from_slice(&0u32.to_le_bytes());
                res
            }
            None => Self {
                id,
                data: vec![
                    0u8;
                    usize::try_from(raptorq.transfer_length)
                        .map_err(|e| Error::TransferLengthTooLarge(e.to_string()))?
                ],
            },
        };
        res.data[0..4].copy_from_slice(&client_id.to_le_bytes());
        res.data[4] = block.serialized();

        if let Some(data) = data {
            let data_len = data.len();
            res.data[5..9].copy_from_slice(&u32::to_le_bytes(
                u32::try_from(data_len).map_err(|e| Error::DataTooLarge(e.to_string()))?,
            ));
            res.data[9..9 + data_len].copy_from_slice(data);
        }

        Ok(res)
    }

    #[must_use]
    pub fn client_id(&self) -> ClientId {
        u32::from_le_bytes([self.data[0], self.data[1], self.data[2], self.data[3]])
    }

    pub fn block_type(&self) -> Result<BlockType, Error> {
        self.data
            .get(4)
            .ok_or(Error::InvalidBlockType(None))
            .and_then(|b| match *b {
                ID_HEARTBEAT => Ok(BlockType::Heartbeat),
                ID_START => Ok(BlockType::Start),
                ID_DATA => Ok(BlockType::Data),
                ID_ABORT => Ok(BlockType::Abort),
                ID_END => Ok(BlockType::End),
                b => Err(Error::InvalidBlockType(Some(b))),
            })
    }

    fn payload_len(&self) -> u32 {
        u32::from_le_bytes([self.data[5], self.data[6], self.data[7], self.data[8]])
    }

    #[must_use]
    pub const fn deserialize(id: u8, data: Vec<u8>) -> Self {
        Self { id, data }
    }

    #[must_use]
    pub const fn max_data_len(raptorq: &RaptorQ) -> usize {
        raptorq.transfer_length as usize - SERIALIZE_OVERHEAD
    }

    #[must_use]
    pub const fn id(&self) -> u8 {
        self.id
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        let len = self.payload_len();
        &self.data[SERIALIZE_OVERHEAD..(SERIALIZE_OVERHEAD + len as usize)]
    }

    #[must_use]
    pub const fn serialized(&self) -> &[u8] {
        self.data.as_slice()
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
            "client {:x} block = {} id = {} data = {} byte(s)",
            self.client_id(),
            msg_type,
            self.id,
            self.payload_len()
        )
    }
}
