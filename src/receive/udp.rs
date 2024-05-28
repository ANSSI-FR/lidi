//! Worker that actually receives packets from the UDP diode link

use crate::sock_utils;
use std::net::{self, SocketAddr};
use std::{io::Result, net::UdpSocket};

// TODO : refactor with send/udp to do
// TODO : remove unwrap

pub struct UdpReceiver {
    socket: UdpSocket,
    mtu: u16,
}

impl UdpReceiver {
    pub fn new(
        from_udp: SocketAddr,
        from_udp_mtu: u16,
        min_buf_size: u64,
        _from_max_messages: u16,
    ) -> Self {
        log::info!(
            "listening for UDP packets at {} with MTU {}",
            from_udp,
            from_udp_mtu
        );
        let mut socket = net::UdpSocket::bind(from_udp).unwrap();

        // set recv buf size to maximum allowed by system conf
        sock_utils::set_socket_recv_buffer_size(&mut socket, i32::MAX).unwrap();

        // check if it is big enough or print warning
        let sock_buffer_size = sock_utils::get_socket_recv_buffer_size(&socket).unwrap();
        log::info!("UDP socket receive buffer size set to {sock_buffer_size}");
        if (sock_buffer_size as u64) < 5 * min_buf_size {
            log::warn!("UDP socket recv buffer is be too small to achieve optimal performances");
            log::warn!("Please modify the kernel parameters using sysctl -w net.core.rmem_max");
        }

        Self {
            socket,
            mtu: from_udp_mtu,
        }
    }

    pub fn recv(&mut self, buffer: &mut [u8]) -> Result<usize> {
        self.socket.recv(buffer)
    }

    pub fn mtu(&self) -> u16 {
        self.mtu
    }
}
