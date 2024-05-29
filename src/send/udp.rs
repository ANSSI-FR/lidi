//! Worker that actually sends packets on the UDP diode link

use crate::{send, sock_utils, udp};
use std::net;

pub(crate) fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
    log::info!(
        "sending UDP traffic to {} with MTU {} binding to {}",
        sender.config.to_udp,
        sender.config.to_mtu,
        sender.config.to_bind
    );
    let socket = net::UdpSocket::bind(sender.config.to_bind)?;
    sock_utils::set_socket_send_buffer_size(&socket, sender.config.udp_buffer_size as i32)?;
    let sock_buffer_size = sock_utils::get_socket_send_buffer_size(&socket)?;
    log::info!("UDP socket send buffer size set to {sock_buffer_size}");
    if (sock_buffer_size as u64)
        < 2 * (sender.config.encoding_block_size + u64::from(sender.config.repair_block_size))
    {
        log::warn!("UDP socket send buffer may be too small to achieve optimal performances");
        log::warn!("Please review the kernel parameters using sysctl");
    }

    let mut udp_messages = udp::UdpMessages::new_sender(
        socket,
        usize::from(sender.to_max_messages),
        sender.config.to_udp,
        sender.config.bandwidth_limit,
    );

    loop {
        let packets = sender.for_send.recv()?;
        udp_messages.send_mmsg(
            packets
                .iter()
                .map(raptorq::EncodingPacket::serialize)
                .collect(),
        )?;
    }
}
