use crate::udp;
use crossbeam_channel::{Receiver, RecvError};
use log::error;
use raptorq::EncodingPacket;
use std::{
    fmt, io,
    net::{SocketAddr, UdpSocket},
};

pub struct Config {
    pub to_bind: SocketAddr,
    pub to_udp: SocketAddr,
    pub mtu: u16,
    pub max_messages: u16,
}

enum Error {
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

pub type Message = EncodingPacket;

pub fn new(config: Config, recvq: Receiver<Vec<Message>>) {
    if let Err(e) = main_loop(config, recvq) {
        error!("UDP send loop error: {e}");
    }
}

fn main_loop(config: Config, recvq: Receiver<Vec<Message>>) -> Result<(), Error> {
    let socket = UdpSocket::bind(config.to_bind)?;
    socket.connect(config.to_udp).unwrap();

    let mut udp_messages = udp::UdpMessages::new_sender(socket, usize::from(config.max_messages));

    loop {
        let packets = recvq.recv()?;
        udp_messages.send_mmsg(packets.iter().map(EncodingPacket::serialize).collect());
    }
}
