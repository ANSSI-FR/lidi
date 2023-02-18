use crate::{sock_utils, udp};
use crossbeam_channel::{Receiver, RecvError};
use log::{error, info};
use raptorq::EncodingPacket;
use std::{
    fmt, io,
    net::{SocketAddr, UdpSocket},
};

pub struct Config {
    pub to_bind: Vec<SocketAddr>,
    pub to_udp: SocketAddr,
    pub mtu: u16,
    pub max_messages: u16,
    pub encoding_block_size: u64,
    pub repair_block_size: u32,
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

pub fn new(config: Config, recvq: &Receiver<Vec<EncodingPacket>>) {
    if let Err(e) = main_loop(config, recvq) {
        error!("UDP send loop error: {e}");
    }
}

fn main_loop(config: Config, recvq: &Receiver<Vec<EncodingPacket>>) -> Result<(), Error> {
    let mut messages = Vec::with_capacity(config.to_bind.len());

    for to_bind in &config.to_bind {
        info!(
            "sending UDP traffic to {} with MTU {} binding to {}",
            config.to_udp, config.mtu, to_bind
        );
        let socket = UdpSocket::bind(to_bind)?;
        sock_utils::set_socket_send_buffer_size(&socket, i32::MAX);
        let sock_buffer_size = sock_utils::get_socket_send_buffer_size(&socket);
        log::info!("UDP socket send buffer size set to {sock_buffer_size}");
        if (sock_buffer_size as u64)
            < 2 * (config.encoding_block_size + config.repair_block_size as u64)
        {
            log::warn!("UDP socket send buffer may be too small to achieve optimal performances");
            log::warn!("Please review the kernel parameters using sysctl");
        }

        let udp_messages =
            udp::UdpMessages::new_sender(socket, usize::from(config.max_messages), config.to_udp);

        messages.push(udp_messages);
    }

    loop {
        let packets = recvq.recv()?;

        let chunks = packets.chunks(config.max_messages as usize);

        let mut i = 0;
        for chunk in chunks {
            messages[i].send_mmsg(chunk.iter().map(EncodingPacket::serialize).collect());
            i = i.wrapping_add(1);
        }
    }
}
