//! Worker that actually sends packets on the UDP diode link

use crate::{send, sock_utils, udp};
use std::{net, os::fd::AsRawFd, thread};

pub(crate) fn start<C>(sender: &send::Sender<C>) -> Result<(), send::Error> {
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
        sender.config.to,
        sender.config.batch_send,
    )?;

    loop {
        let packets = sender.for_send.recv()?;

        udp.send(packets)?;

        thread::yield_now();
    }
}
