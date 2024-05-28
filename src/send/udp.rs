//! Worker that actually sends packets on the UDP diode link

use crate::{protocol::Header, sock_utils};

use std::net::{self, SocketAddr, UdpSocket};

pub struct UdpSender {
    //udp_messages: UdpMessages<udp::UdpSend>,
    socket: UdpSocket,
    buffer: Vec<u8>,
}

impl UdpSender {
    pub fn new(to_bind: SocketAddr, to_udp: SocketAddr, min_buf_size: u64) -> Self {
        let mut socket = net::UdpSocket::bind(to_bind).unwrap();
        sock_utils::set_socket_send_buffer_size(&mut socket, i32::MAX).unwrap();
        let sock_buffer_size = sock_utils::get_socket_send_buffer_size(&socket).unwrap();
        log::info!("UDP socket send buffer size set to {sock_buffer_size}");
        if (sock_buffer_size as u64) < 2 * min_buf_size {
            log::warn!("UDP socket send buffer may be too small to achieve optimal performances");
            log::warn!("Please review the kernel parameters using sysctl");
        }
        //let udp_messages = UdpMessages::new_sender(socket, usize::from(max_messages), to_udp);
        //
        socket.connect(to_udp).unwrap();

        Self {
            socket,
            buffer: vec![0; 64 * 1024],
        }
    }

    pub fn send(&mut self, header: Header, payload: Vec<u8>) -> std::io::Result<()> {
        log::trace!(
            "udp: send session {} block {} seq {} flags {} len {}",
            header.session(),
            header.block(),
            header.seq(),
            header.message_type(),
            payload.len()
        );

        let payload_len = payload.len();

        self.buffer[0..4].copy_from_slice(&header.serialized());
        self.buffer[4..payload_len + 4].copy_from_slice(&payload);

        self.socket.send(&self.buffer[0..payload_len + 4])?;

        Ok(())
    }
}
