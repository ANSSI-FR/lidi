//! Worker that actually receives packets from the UDP diode link

use crate::{receive, sock_utils, udp};
use std::{net, os::fd::AsRawFd};

pub(crate) fn start<F>(receiver: &receive::Receiver<F>) -> Result<(), receive::Error> {
    log::info!(
        "listening for UDP packets at {} with MTU {}",
        receiver.config.from,
        receiver.config.from_mtu,
    );

    let socket = net::UdpSocket::bind(receiver.config.from)?;
    socket.set_nonblocking(false)?;

    let buffer_size = i32::from(super::reblock::WINDOW_WIDTH)
        * i32::try_from(receiver.raptorq.nb_packets())
            .map_err(|e| receive::Error::Other(format!("nb_packets: {e}")))?
        * i32::from(receiver.config.from_mtu);
    sock_utils::set_socket_recv_buffer_size(&socket, buffer_size)?;
    let sock_buffer_size = sock_utils::get_socket_recv_buffer_size(&socket)?;
    log::info!("UDP socket receive buffer size set to {sock_buffer_size}");

    if (sock_buffer_size as i32) < buffer_size {
        log::warn!(
            "UDP socket recv buffer may be too small ({sock_buffer_size} < {buffer_size}) to achieve optimal performances"
        );
        log::warn!("Please review the kernel parameters using sysctl");
    }

    let mut udp = udp::Receive::new(
        socket.as_raw_fd(),
        receiver.config.from_mtu,
        receiver.config.batch_receive,
    );

    loop {
        if receiver.broken_pipeline.load() {
            return Ok(());
        }

        let datagrams = udp.recv()?;
        receiver.to_reblock.send(datagrams)?;
    }
}
