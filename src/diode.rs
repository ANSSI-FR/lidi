use serde::{Deserialize, Serialize};
use std::{fmt, env};

#[derive(Serialize, Deserialize)]
pub enum Message {
    Start,
    Data(Vec<u8>),
    Abort,
    End,
    Padding(Vec<u8>),
}

impl fmt::Display for Message {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Start => write!(fmt, "Start"),
            Self::Data(d) => write!(fmt, "Data({} bytes)", d.len()),
            Self::Abort => write!(fmt, "Abort"),
            Self::End => write!(fmt, "End"),
            Self::Padding(p) => write!(fmt, "Padding({} bytes)", p.len()),
        }
    }
}

pub type ClientId = u32;

pub fn new_client_id() -> ClientId {
    rand::random::<ClientId>()
}

#[derive(Serialize, Deserialize)]
pub struct ClientMessage {
    pub client_id: ClientId,
    pub payload: Message,
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
