//! Definition of the Lidi protocol used to transfer data over UDP
//!
//! The Lidi protocol is rather simple: since the communications are unidirectional, it is defined
//! by the messages structure. There are 5 message types:
//! - `MessageType::Heartbeat` message without data to tell receiver the other side is alive
//! - `MessageType::Start` informs the receiver that the sent data chunk represents the beginning of a new transfer,
//! - `MessageType::Data` is used to inform this packet contains data
//! - `MessageType::End` informs the receiver that the current transfer is completed (i.e. this is
//! the last message for the current connection)
//!
//! A message is stored in a `Vec` of `u8`s, with the following representation:
//!
//! ```text
//!
//!  <--- 1 byte ---> <-- 1 byte --> <-- 2 bytes ->
//! +----------------+--------------+--------------+-----------------------------------------------------------+
//! |                |              |              |                                                           |
//! |  message_flags |  session_id  |    seq_id    |  payload = 4 bytes data length + data + optional padding  |
//! |                |              |              |                                                           |
//! +----------------+--------------+--------------+-----------------------------------------------------------+
//!  <------------ SERIALIZE_OVERHEAD ------------> <------------------- message_length -------------------->
//!
//! ```
//!
//! 4-bytes values are encoded in little-endian byte order.
//!
//! In `Heartbeat` messages, `client_id` is unused and should be set to 0 by the constructor
//! caller. Also no data payload should be provided by the constructor caller in case the message
//! is of type `Heartbeat`, `Abort` or `End`. Then the `data_length` will be set to 0 by the
//! message constructor and the data chunk will be fully padded with zeros.

use crate::error::Error;
use bitflags::bitflags;
use std::fmt;

pub struct DecodedBlock {
    pub header: Header,
    pub block: Vec<u8>,
}

impl DecodedBlock {
    pub fn new(header: Header, block: Vec<u8>) -> Self {
        Self { header, block }
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(Copy, Clone, PartialEq, Eq)]
    pub struct MessageType: u8 {
        const Heartbeat = 0b00000001;
        const Start     = 0b00000010;
        const Data      = 0b00000100;
        const End       = 0b00001000;
        const Abort     = 0b00010000;
        const Init      = 0b10000000;
    }
}

fn display_bit(
    fmt: &mut fmt::Formatter<'_>,
    flags: MessageType,
    flag: MessageType,
    name: &str,
    count: &mut usize,
) -> Result<(), fmt::Error> {
    if flags.contains(flag) {
        if *count != 0 {
            write!(fmt, "|")?;
        }
        write!(fmt, "{name}")?;
        *count += 1;
    }

    Ok(())
}

impl fmt::Display for MessageType {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let mut count = 0;

        display_bit(fmt, *self, MessageType::Heartbeat, "Heartbeat", &mut count)?;
        display_bit(fmt, *self, MessageType::Start, "Start", &mut count)?;
        display_bit(fmt, *self, MessageType::Data, "Data", &mut count)?;
        display_bit(fmt, *self, MessageType::End, "End", &mut count)?;
        display_bit(fmt, *self, MessageType::Init, "Init", &mut count)?;

        Ok(())
    }
}

#[derive(Copy, Clone)]
pub struct Header {
    flags: MessageType,
    session: u8,
    seq: u8,
    block: u8,
}

const SERIALIZE_OVERHEAD: u16 = 4;
/// data added to each block to store real data size (without protocol padding)
pub const PAYLOAD_OVERHEAD: usize = 4;
pub const FIRST_BLOCK_ID: u8 = 0;
pub const FIRST_SESSION_ID: u8 = 0;

impl Header {
    /// Message constructor, craft a message according to the representation introduced in
    /// [crate::protocol].
    ///
    /// Some (unchecked) constraints on arguments must be respected:
    /// - if `message` is `MessageType::Heartbeat`, `MessageType::Abort` or `MessageType::End`
    /// then no data should be provided,
    /// - if `message` is `MessageType::Heartbear` then `client_id` should be equal to 0,
    /// - if there is some `data`, its length must be greater than `message_length`.
    pub fn new(flags: MessageType, session: u8, block: u8) -> Self {
        Self {
            flags,
            session,
            block,
            seq: 0,
        }
    }

    pub fn message_type(&self) -> MessageType {
        self.flags
    }

    pub(crate) const fn deserialize(data: &[u8]) -> Result<Header, Error> {
        // very unlikely
        if data.len() < SERIALIZE_OVERHEAD as usize {
            return Err(Error::UdpHeaderDeserialize);
        }

        // check flags
        if let Some(flags) = MessageType::from_bits(data[0]) {
            let session = data[1];
            let block = data[2];
            let seq = data[3];
            Ok(Header {
                flags,
                session,
                block,
                seq,
            })
        } else {
            Err(Error::UdpHeaderDeserialize)
        }
    }

    pub const fn serialize_overhead() -> usize {
        SERIALIZE_OVERHEAD as _
    }

    pub fn serialized(&self) -> [u8; 4] {
        let mut data: [u8; 4] = [0; 4];
        data[0] = self.flags.bits();
        data[1] = self.session;
        data[2] = self.block;
        data[3] = self.seq;
        data
    }

    pub fn block(&self) -> u8 {
        self.block
    }

    pub fn seq(&self) -> u8 {
        self.seq
    }

    pub fn session(&self) -> u8 {
        self.session
    }

    pub fn incr_seq(&mut self) {
        self.seq += 1;
    }
}

impl fmt::Display for Header {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            fmt,
            "client message = {} session {} seq {}",
            self.message_type(),
            self.session,
            self.seq
        )
    }
}

const PACKET_HEADER_SIZE: u16 = 20 + 8;
const RAPTORQ_ALIGNMENT: u16 = 8;
const RAPTORQ_HEADER_SIZE: u16 = 4;

pub fn object_transmission_information(
    mtu: u16,
    logical_block_size: u64,
) -> raptorq::ObjectTransmissionInformation {
    let data_mtu: u16 = RAPTORQ_ALIGNMENT
        * ((mtu - PACKET_HEADER_SIZE - RAPTORQ_HEADER_SIZE - SERIALIZE_OVERHEAD)
            / RAPTORQ_ALIGNMENT);

    let nb_encoding_packets = (logical_block_size + PAYLOAD_OVERHEAD as u64) / u64::from(data_mtu);

    let encoding_block_size = u64::from(data_mtu) * nb_encoding_packets;

    let data_mtu = (encoding_block_size / nb_encoding_packets) as u16;

    raptorq::ObjectTransmissionInformation::with_defaults(encoding_block_size, data_mtu)
}

pub(crate) fn data_mtu(oti: &raptorq::ObjectTransmissionInformation) -> u16 {
    oti.symbol_size()
}

pub fn packet_size(oti: &raptorq::ObjectTransmissionInformation) -> u16 {
    (oti.transfer_length() / nb_encoding_packets(oti)) as u16
}

pub fn nb_encoding_packets(oti: &raptorq::ObjectTransmissionInformation) -> u64 {
    oti.transfer_length() / u64::from(data_mtu(oti))
}

pub fn nb_repair_packets(
    oti: &raptorq::ObjectTransmissionInformation,
    repair_block_size: u32,
) -> u32 {
    repair_block_size / u32::from(data_mtu(oti))
}
