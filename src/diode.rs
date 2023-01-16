use std::{env, fmt, io};

pub enum Message {
    Start,
    Data(Vec<u8>),
    Abort,
    End,
    Padding(u32),
}

pub enum Error {
    Io(io::Error),
    Serialization(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Serialization(e) => write!(fmt, "serialization error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

const ID_START: u8 = 0x01;
const ID_DATA: u8 = 0x02;
const ID_ABORT: u8 = 0x03;
const ID_END: u8 = 0x04;
const ID_PADDING: u8 = 0x05;

impl Message {
    pub fn deserialize_from<R: io::Read>(r: &mut R) -> Result<Self, Error> {
        let mut kind_buf = [0u8; 1];
        r.read_exact(&mut kind_buf)?;
        match kind_buf[0] {
            ID_START => Ok(Self::Start),
            ID_DATA => {
                let mut len_buf = [0u8; 8];
                r.read_exact(&mut len_buf)?;
                let len = u64::from_le_bytes(len_buf) as usize;
                let mut data_buf = vec!(0; len);
                r.read_exact(&mut data_buf)?;
                Ok(Self::Data(data_buf))
            }
            ID_ABORT => Ok(Self::Abort),
            ID_END => Ok(Self::End),
            ID_PADDING => {
                let mut padlen_buf = [0u8; 4];
                r.read_exact(&mut padlen_buf)?;
                let padlen = u32::from_le_bytes(padlen_buf);
                let mut padding = vec!(0; padlen as usize);
                r.read_exact(&mut padding)?;
                Ok(Self::Padding(padlen))
            }
            v => Err(Error::Serialization(format!("unexcepted value 0x{:x}", v))),
        }
    }

    pub(crate) fn serialize_padding_overhead() -> usize {
        1 + 4
    }

    pub fn serialize_to<W: io::Write>(&self, w: &mut W) -> Result<(), Error> {
        match self {
            Self::Start => w.write_all(&ID_START.to_le_bytes())?,
            Self::Data(data) => {
                w.write_all(&ID_DATA.to_le_bytes())?;
                w.write_all(&data.len().to_le_bytes())?;
                w.write_all(data)?;
            },
            Self::Abort => w.write_all(&ID_ABORT.to_le_bytes())?,
            Self::End => w.write_all(&ID_END.to_le_bytes())?,
            Self::Padding(padlen) => {
                w.write_all(&ID_PADDING.to_le_bytes())?;
                w.write_all(&padlen.to_le_bytes())?;
                let padding = vec!(0; *padlen as usize);
                w.write_all(&padding)?;
            },
        };
        Ok(())
    }
}

impl fmt::Display for Message {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Start => write!(fmt, "Start"),
            Self::Data(d) => write!(fmt, "Data({} bytes)", d.len()),
            Self::Abort => write!(fmt, "Abort"),
            Self::End => write!(fmt, "End"),
            Self::Padding(p) => write!(fmt, "Padding({} bytes)", p),
        }
    }
}

pub type ClientId = u32;

pub fn new_client_id() -> ClientId {
    rand::random::<ClientId>()
}

pub struct ClientMessage {
    pub client_id: ClientId,
    pub payload: Message,
}

impl ClientMessage {
    pub fn deserialize_from<R: io::Read>(r: &mut R) -> Result<Self, Error> {
        let mut id_buf = [0u8; 4];
        r.read_exact(&mut id_buf)?;
        let client_id = u32::from_le_bytes(id_buf);
        let payload = Message::deserialize_from(r)?;
        Ok(Self { client_id, payload })
    }

    pub fn serialize_padding_overhead() -> usize {
        4 + Message::serialize_padding_overhead()
    }

    pub fn serialize_to<W: io::Write>(&self, w: &mut W) -> Result<(), Error> {
        w.write_all(&self.client_id.to_le_bytes())?;
        self.payload.serialize_to(w)
    }
}

impl fmt::Display for ClientMessage {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(fmt, "client {:x} {}", self.client_id, self.payload)
    }
}

pub fn adjust_encoding_block_size(mtu: u16, encoding_block_size: u64) -> u64 {
    let payload_size = 4;
    (mtu as u64 - payload_size) * (encoding_block_size / (mtu as u64 - payload_size))
}

pub fn adjust_repair_block_size(mtu: u16, repair_block_size: u32) -> u32 {
    let payload_size = 4;
    (mtu as u32 - payload_size) * (repair_block_size / (mtu as u32 - payload_size))
}

pub fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env().unwrap();
    } else {
        simple_logger::init_with_level(log::Level::Info).unwrap();
    }
}
