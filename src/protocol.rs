//! Definition of the Lidi protocol used to transfer data over UDP
//!
//! The Lidi protocol is rather simple: since the communications are unidirectional, it is defined
//! by the messages structure. There are 5 message types:
//! - `MessageType::Heartbeat` lets know the receiver that transfer can happen,
//! - `MessageType::Start` informs the receiver that the sent data chunk represents the beginning of a new transfer,
//! - `MessageType::Data` is used to send a data chunk that is not the beginning nor the ending of
//! a transfer,
//! - `MessageType::Abort` informs the receiver that the current transfer has been aborted on the
//! sender side,
//! - `MessageType::End` informs the receiver that the current transfer is completed (i.e. all
//! data have been sent).
//!
//! A message is stored in a `Vec` of `u8`s, with the following representation:
//!
//! ```text
//!
//! <-- 4 bytes -> <--- 1 byte ---> <-- 4 bytes -->
//! --------------+----------------+---------------+--------------------------------------
//! |             |                |               |                                     |
//! |  client_id  |  message_type  |  data_length  |  payload = data + optional padding  |
//! |             |                |               |                                     |
//! --------------+----------------+---------------+--------------------------------------
//!  <------------ SERIALIZE_OVERHEAD ------------> <--------- message_length ---------->
//!
//! ```
//!
//! 4-bytes values are encoded in little-endian byte order.
//!
//! In `Heartbeat` messages, `client_id` is unused and should be set to 0 by the constructor
//! caller. Also no data payload should be provided by the constructor caller in case the message
//! is of type `Heartbeat`, `Abort` or `End`. Then the `data_length` will be set to 0 by the
//! message constructor and the data chunk will be fully padded with zeros.

use std::{fmt, io};

pub enum Error {
    Io(io::Error),
    InvalidMessageType(Option<u8>),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::InvalidMessageType(b) => write!(fmt, "invalid message type: {b:?}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub(crate) enum MessageType {
    Heartbeat,
    Start,
    Data,
    Abort,
    End,
}

impl MessageType {
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

impl fmt::Display for MessageType {
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

pub(crate) type ClientId = u32;

pub(crate) fn new_client_id() -> ClientId {
    rand::random::<ClientId>()
}

pub struct Message(Vec<u8>);

const SERIALIZE_OVERHEAD: usize = 4 + 1 + 4;

impl Message {
    /// Message constructor, craft a message according to the representation introduced in
    /// [crate::protocol].
    ///
    /// Some (unchecked) constraints on arguments must be respected:
    /// - if `message` is `MessageType::Heartbeat`, `MessageType::Abort` or `MessageType::End`
    /// then no data should be provided,
    /// - if `message` is `MessageType::Heartbeat` then `client_id` should be equal to 0,
    /// - if there is some `data`, its length must be greater than `message_length`.
    pub(crate) fn new(
        message: MessageType,
        message_length: u32,
        client_id: ClientId,
        data: Option<&[u8]>,
    ) -> Self {
        match data {
            None => {
                let mut content = vec![0u8; message_length as usize + SERIALIZE_OVERHEAD];
                let bytes = client_id.to_le_bytes();
                content[0] = bytes[0];
                content[1] = bytes[1];
                content[2] = bytes[2];
                content[3] = bytes[3];
                content[4] = message.serialized();
                Self(content)
            }
            Some(data) => {
                let mut content = Vec::with_capacity(message_length as usize + SERIALIZE_OVERHEAD);
                content.extend_from_slice(&client_id.to_le_bytes());
                content.push(message.serialized());
                content.extend_from_slice(&u32::to_le_bytes(data.len() as u32));
                content.extend_from_slice(data);
                if content.len() < content.capacity() {
                    content.resize(content.capacity(), 0);
                }
                Self(content)
            }
        }
    }

    pub(crate) fn client_id(&self) -> ClientId {
        let bytes = [self.0[0], self.0[1], self.0[2], self.0[3]];
        u32::from_le_bytes(bytes)
    }

    pub(crate) fn message_type(&self) -> Result<MessageType, Error> {
        match self.0.get(4) {
            Some(&ID_HEARTBEAT) => Ok(MessageType::Heartbeat),
            Some(&ID_START) => Ok(MessageType::Start),
            Some(&ID_DATA) => Ok(MessageType::Data),
            Some(&ID_ABORT) => Ok(MessageType::Abort),
            Some(&ID_END) => Ok(MessageType::End),
            b => Err(Error::InvalidMessageType(b.copied())),
        }
    }

    fn payload_len(&self) -> u32 {
        let data_len_bytes = [self.0[5], self.0[6], self.0[7], self.0[8]];
        u32::from_le_bytes(data_len_bytes)
    }

    pub(crate) const fn deserialize(data: Vec<u8>) -> Self {
        Self(data)
    }

    pub const fn serialize_overhead() -> usize {
        SERIALIZE_OVERHEAD
    }

    pub(crate) fn payload(&self) -> &[u8] {
        let len = self.payload_len();
        &self.0[SERIALIZE_OVERHEAD..(SERIALIZE_OVERHEAD + len as usize)]
    }

    pub(crate) fn serialized(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for Message {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            fmt,
            "client {:x} message = {} data = {} byte(s)",
            self.client_id(),
            self.message_type().map_err(|_| fmt::Error)?,
            self.payload_len()
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
    let data_mtu: u16 =
        RAPTORQ_ALIGNMENT * ((mtu - PACKET_HEADER_SIZE - RAPTORQ_HEADER_SIZE) / RAPTORQ_ALIGNMENT);

    let nb_encoding_packets = logical_block_size / u64::from(data_mtu);

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
