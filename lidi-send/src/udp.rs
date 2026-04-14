//! Worker that encodes protocol blocks into `RaptorQ` packets

use crate::socket;
use std::{
    io,
    net::{self, ToSocketAddrs},
};

pub fn start<C>(sender: &crate::Sender<C>, to_port: u16) -> Result<(), crate::Error> {
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

    let addresses = (sender.config.to.as_str(), 0)
        .to_socket_addrs()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                format!("bad IP or hostname {:?}: {e}", sender.config.to),
            )
        })?
        .filter(net::SocketAddr::is_ipv4)
        .collect::<Vec<_>>();
    let address = if addresses.len() == 1 {
        addresses[0].ip()
    } else {
        return Err(crate::Error::Io(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("hostname matches several addresses for UDP destination: {addresses:?}"),
        )));
    };

    log::info!(
        "sending UDP traffic to {}:{} with MTU {} binding to {}",
        address,
        to_port,
        sender.config.mtu,
        sender.config.to_bind
    );

    let address = net::SocketAddr::new(address, to_port);

    let mut udp = socket::Send::new(socket, address, sender.config.mode)?;

    loop {
        let Some(block) = sender.for_udp.recv()? else {
            return Ok(());
        };

        let block_id = block.id();
        let client_id = block.client_id();

        log::trace!("encoding block {block_id} for client {client_id:x}");

        let packets = sender.raptorq.encode(block_id, block.serialized());

        sender.block_recycler.push(block);

        log::debug!("sending block {block_id} ({} packets)", packets.len());

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
