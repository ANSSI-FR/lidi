//! Worker that encodes protocol blocks into `RaptorQ` packets

use crate::socket;
use std::net;

pub fn start<C>(sender: &crate::Sender<C>, to_port: u16) -> Result<(), crate::Error> {
    log::info!(
        "sending UDP traffic to {} with MTU {} binding to {}",
        sender.config.to,
        sender.config.mtu,
        sender.config.to_bind
    );

    let socket = net::UdpSocket::bind(sender.config.to_bind)?;
    socket.set_nonblocking(false)?;

    let buffer_size = sender.raptorq.nb_packets() * u32::from(sender.config.mtu);
    let buffer_size = i32::try_from(buffer_size)
        .map_err(|e| crate::Error::Internal(format!("too large buffer size: {e}")))?;

    socket::set_socket_send_buffer_size(&socket, buffer_size)?;
    let sock_buffer_size = socket::get_socket_send_buffer_size(&socket)?;
    log::info!("UDP socket send buffer size set to {sock_buffer_size}");

    if sock_buffer_size < buffer_size {
        log::warn!(
            "UDP socket send buffer may be too small ({sock_buffer_size} < {buffer_size}) to achieve optimal performances"
        );
        log::warn!("Please review the kernel parameters using sysctl");
    }

    let mut udp = socket::Send::new(
        socket,
        net::SocketAddr::new(sender.config.to, to_port),
        sender.config.mode,
    )?;

    loop {
        let Some((id, block)) = sender.for_udp.recv()? else {
            return Ok(());
        };

        let client_id = block.client_id();

        log::debug!("encoding block {id} for client {client_id:x}");

        let packets = sender.raptorq.encode(id, block.serialized());

        log::debug!("sending block {id}");

        if let Err(e) = udp.send(&packets) {
            log::error!("failed to send UDP packet: {e}");
            #[cfg(feature = "prometheus")]
            metrics::counter!("lidi_error_udp_packets").increment(packets.len() as u64);
        } else {
            #[cfg(feature = "prometheus")]
            metrics::counter!("lidi_send_udp_packets").increment(packets.len() as u64);
        }
    }
}
