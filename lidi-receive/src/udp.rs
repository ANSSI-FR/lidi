//! Worker that actually receives packets from the UDP diode link

use crate::socket;
use std::net;

pub fn start<ClientNew, ClientEnd>(
    receiver: &crate::Receiver<ClientNew, ClientEnd>,
    port: u16,
) -> Result<(), crate::Error> {
    log::info!(
        "listening for UDP packets at {}:{} with MTU {}",
        receiver.config.from,
        port,
        receiver.config.mtu,
    );

    let socket = net::UdpSocket::bind(net::SocketAddr::new(receiver.config.from, port))?;
    socket.set_nonblocking(false)?;

    let buffer_size = u32::from(super::reblock::WINDOW_WIDTH)
        * receiver.raptorq.nb_packets()
        * u32::from(receiver.config.mtu);
    let buffer_size = i32::try_from(buffer_size)
        .map_err(|e| crate::Error::Internal(format!("nb_packets: {e}")))?;

    socket::set_socket_recv_buffer_size(&socket, buffer_size)?;
    let sock_buffer_size = socket::get_socket_recv_buffer_size(&socket)?;
    log::info!("UDP socket receive buffer size set to {sock_buffer_size}");

    if sock_buffer_size < buffer_size {
        log::warn!(
            "UDP socket recv buffer may be too small ({sock_buffer_size} < {buffer_size}) to achieve optimal performances"
        );
        log::warn!("Please review the kernel parameters using sysctl");
    }

    let mut udp = socket::Receive::new(socket, receiver.config.mtu, receiver.config.mode)?;

    loop {
        match udp.recv()? {
            #[cfg(any(feature = "receive-native", feature = "receive-msg"))]
            socket::ReceiveDatagrams::Single(datagram) => {
                #[cfg(feature = "prometheus")]
                metrics::counter!("lidi_receive_udp_packets").increment(1);
                let packet = raptorq::EncodingPacket::deserialize(datagram);
                #[cfg(not(feature = "receive-mmsg"))]
                receiver.to_reblock.send(packet)?;
                #[cfg(feature = "receive-mmsg")]
                receiver.to_reblock.send(vec![packet])?;
            }
            #[cfg(feature = "receive-mmsg")]
            socket::ReceiveDatagrams::Multiple(datagrams) => {
                #[cfg(feature = "prometheus")]
                metrics::counter!("lidi_receive_udp_packets").increment(datagrams.len() as u64);
                receiver.to_reblock.send(
                    datagrams
                        .into_iter()
                        .map(raptorq::EncodingPacket::deserialize)
                        .collect(),
                )?;
            }
        }
    }
}
