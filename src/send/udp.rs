//! Worker that encodes protocol blocks into `RaptorQ` packets

use crate::{send, sock_utils, udp};
use std::{net, os::fd::AsRawFd};

pub fn start<C>(sender: &send::Sender<C>, to_port: u16) -> Result<(), send::Error> {
    log::info!(
        "sending UDP traffic to {} with MTU {} binding to {}",
        sender.config.to,
        sender.config.to_mtu,
        sender.config.to_bind
    );

    let socket = net::UdpSocket::bind(sender.config.to_bind)?;
    socket.set_nonblocking(false)?;

    let buffer_size = i32::try_from(sender.raptorq.nb_packets())
        .map_err(|e| send::Error::Other(format!("nb_packets: {e}")))?
        * i32::from(sender.config.to_mtu);
    sock_utils::set_socket_send_buffer_size(&socket, buffer_size)?;
    let sock_buffer_size = sock_utils::get_socket_send_buffer_size(&socket)?;
    log::info!("UDP socket send buffer size set to {sock_buffer_size}");

    if (sock_buffer_size as i32) < buffer_size {
        log::warn!(
            "UDP socket send buffer may be too small ({sock_buffer_size} < {buffer_size}) to achieve optimal performances"
        );
        log::warn!("Please review the kernel parameters using sysctl");
    }

    let mut udp = udp::Send::new(
        socket.as_raw_fd(),
        net::SocketAddr::new(sender.config.to, to_port),
        sender.config.batch_send,
    )?;

    loop {
        let Some((id, block)) = sender.for_udp.recv()? else {
            return Ok(());
        };

        let client_id = block.client_id();

        log::debug!("encoding block {id} for client {client_id:x}");

        let packets = sender.raptorq.encode(id, block.serialized());

        log::debug!("sending block {id}");

        udp.send(&packets)?;
    }
}
