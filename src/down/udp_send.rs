use crossbeam_channel::{Receiver, RecvError};
use log::error;
use raptorq::EncodingPacket;
use std::{
    fmt, io,
    net::{SocketAddr, UdpSocket},
};

pub(crate) struct Config {
    pub to_udp: SocketAddr,
    pub mtu: u16,
}

pub(crate) enum Error {
    Io(io::Error),
    Crossbeam(RecvError),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Crossbeam(e) => write!(fmt, "crossbeam error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<RecvError> for Error {
    fn from(e: RecvError) -> Self {
        Self::Crossbeam(e)
    }
}

pub(crate) type Message = EncodingPacket;

pub(crate) fn new(config: Config, recvq: Receiver<Message>) {
    if let Err(e) = main_loop(config, recvq) {
        error!("UDP send loop error: {e}");
    }
}

fn main_loop(config: Config, recvq: Receiver<Message>) -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;

    loop {
        let packet = recvq.recv()?;

        socket.send_to(&packet.serialize(), config.to_udp)?;
    }
}
