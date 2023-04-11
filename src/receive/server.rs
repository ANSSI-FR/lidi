use std::net;

use crate::{sock_utils, udp};

pub(crate) fn start<F>(receiver: &super::Receiver<F>) -> Result<(), super::Error> {
    log::info!(
        "listening for UDP packets at {} with MTU {}",
        receiver.config.from_udp,
        receiver.config.from_udp_mtu
    );
    let socket = net::UdpSocket::bind(receiver.config.from_udp)?;
    sock_utils::set_socket_recv_buffer_size(&socket, i32::MAX)?;
    let sock_buffer_size = sock_utils::get_socket_recv_buffer_size(&socket)?;
    log::info!("UDP socket receive buffer size set to {sock_buffer_size}");
    if (sock_buffer_size as u64)
        < 2 * (receiver.config.encoding_block_size + receiver.config.repair_block_size as u64)
    {
        log::warn!("UDP socket recv buffer may be too small to achieve optimal performances");
        log::warn!("Please review the kernel parameters using sysctl");
    }

    let mut udp_messages = udp::UdpMessages::new_receiver(
        socket,
        usize::from(receiver.from_max_messages),
        usize::from(receiver.config.from_udp_mtu),
    );

    loop {
        let packets = udp_messages
            .recv_mmsg()?
            .map(raptorq::EncodingPacket::deserialize);
        receiver.to_reblock.send(packets.collect())?;
    }
}
