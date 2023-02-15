use std::{fmt, io};

pub(crate) enum Error {
    Io(io::Error),
    InvalidMessageType(u8),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::InvalidMessageType(b) => write!(fmt, "invalid message type: 0x{b:x}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub(crate) enum MessageType {
    Start,
    Data,
    Abort,
    End,
}

impl MessageType {
    fn serialized(self) -> u8 {
        match self {
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
            Self::Start => write!(fmt, "Start"),
            Self::Data => write!(fmt, "Data"),
            Self::Abort => write!(fmt, "Abort"),
            Self::End => write!(fmt, "End"),
        }
    }
}

const ID_START: u8 = 0x01;
const ID_DATA: u8 = 0x02;
const ID_ABORT: u8 = 0x03;
const ID_END: u8 = 0x04;

pub(crate) type ClientId = u32;

pub(crate) fn new_client_id() -> ClientId {
    rand::random::<ClientId>()
}

pub struct ClientMessage(Vec<u8>);

const SERIALIZE_OVERHEAD: usize = 4 + 1 + 4;

impl ClientMessage {
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
        match self.0[4] {
            ID_START => Ok(MessageType::Start),
            ID_DATA => Ok(MessageType::Data),
            ID_ABORT => Ok(MessageType::Abort),
            ID_END => Ok(MessageType::End),
            b => Err(Error::InvalidMessageType(b)),
        }
    }

    fn payload_len(&self) -> u32 {
        let data_len_bytes = [self.0[5], self.0[6], self.0[7], self.0[8]];
        u32::from_le_bytes(data_len_bytes)
    }

    pub(crate) fn deserialize(data: Vec<u8>) -> Self {
        Self(data)
    }

    pub fn serialize_overhead() -> usize {
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

impl fmt::Display for ClientMessage {
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

    let nb_encoding_packets = logical_block_size / data_mtu as u64;

    let encoding_block_size = data_mtu as u64 * nb_encoding_packets;

    let data_mtu = (encoding_block_size / nb_encoding_packets) as u16;

    raptorq::ObjectTransmissionInformation::with_defaults(encoding_block_size, data_mtu)
}

pub(crate) fn data_mtu(oti: &raptorq::ObjectTransmissionInformation) -> u16 {
    oti.symbol_size()
}

pub(crate) fn nb_encoding_packets(oti: &raptorq::ObjectTransmissionInformation) -> u64 {
    oti.transfer_length() / data_mtu(oti) as u64
}
