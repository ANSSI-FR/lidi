use crate::protocol::{DecodedBlock, Header};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("UDP packet header deserialize")]
    UdpHeaderDeserialize,
    #[error("Send block")]
    CrossbeamSendBlock(#[from] crossbeam_channel::SendError<DecodedBlock>),
    #[error("Send block")]
    CrossbeamSendEncoding(#[from] crossbeam_channel::SendError<(Header, Vec<u8>)>),
    #[error("Receive block")]
    CrossbeamReceiver(#[from] crossbeam_channel::RecvError),
    #[error("Io")]
    Io(#[from] std::io::Error),
}
