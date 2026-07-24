//! Worker that actually receives packets from the UDP diode link

use crate::{ClientLifecycle, reblock, socket};
use lidi_protocol as protocol;
use std::{
    io,
    net::{self, ToSocketAddrs},
};

// Pops a drained batch sent back by reblock, falling back to a fresh, empty `Vec` if none is
// available yet (e.g. at start-up), mirroring lidi-send's block_recycler.
#[cfg(feature = "receive-mmsg")]
fn take_recycled<Lifecycle>(receiver: &crate::Receiver<Lifecycle>) -> Vec<raptorq::EncodingPacket>
where
    Lifecycle: ClientLifecycle,
{
    receiver
        .packet_vec_recycler
        .steal()
        .success()
        .unwrap_or_default()
}

// Updates `*session_id` and notifies reblock whenever a datagram's session id doesn't match the
// one currently tracked (which also covers the very first datagram, since `session_id` starts
// at 0 and real session ids never are).
fn track_session(
    session_id: &mut protocol::SessionId,
    datagram_session_id: protocol::SessionId,
    to_reblock: &crossbeam_channel::Sender<reblock::Message>,
) -> Result<(), crate::Error> {
    if *session_id == 0 {
        *session_id = datagram_session_id;
        log::debug!("session is {session_id:x}");
        to_reblock.send(reblock::Message::NewSession(*session_id))?;
    } else if datagram_session_id != *session_id {
        *session_id = datagram_session_id;
        log::debug!("new session is {session_id:x}");
        to_reblock.send(reblock::Message::NewSession(*session_id))?;
    }
    Ok(())
}

pub fn start<Lifecycle>(
    receiver: &crate::Receiver<Lifecycle>,
    port: u16,
    to_reblock: &crossbeam_channel::Sender<reblock::Message>,
) -> Result<(), crate::Error>
where
    Lifecycle: ClientLifecycle,
{
    let addresses = (receiver.config.from.as_str(), 0)
        .to_socket_addrs()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                format!("bad IP or hostname {:?}: {e}", receiver.config.from),
            )
        })?
        .filter(net::SocketAddr::is_ipv4)
        .collect::<Vec<_>>();
    let address = if addresses.len() == 1 {
        addresses[0].ip()
    } else {
        return Err(crate::Error::Io(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("hostname matches several addresses for UDP source: {addresses:?}"),
        )));
    };

    log::info!(
        "listening for UDP packets at {}:{} with MTU {}",
        address,
        port,
        receiver.config.mtu,
    );

    let socket = net::UdpSocket::bind((address, port))?;
    socket.set_nonblocking(false)?;

    let buffer_size = u32::from(super::reblock::WINDOW_WIDTH)
        * receiver.raptorq.nb_packets()
        * u32::from(receiver.config.mtu);
    let buffer_size = i32::try_from(buffer_size)
        .map_err(|e| crate::Error::Internal(format!("nb_packets: {e}")))?;

    if let Err(e) = socket::set_socket_recv_buffer_size(&socket, buffer_size) {
        log::warn!("failed to set socket recv buffer size: {e}");
    }
    let sock_buffer_size = socket::get_socket_recv_buffer_size(&socket)?;
    log::info!("UDP socket receive buffer size set to {sock_buffer_size}");

    if sock_buffer_size < buffer_size {
        log::warn!(
            "UDP socket recv buffer may be too small ({sock_buffer_size} < {buffer_size}) to achieve optimal performances"
        );
        log::warn!("Please review the kernel parameters using sysctl");
    }

    let mut session_id = 0;

    let mut udp = socket::Receive::new(socket, receiver.config.mtu, receiver.config.mode)?;

    loop {
        match udp.recv()? {
            #[cfg(any(feature = "receive-native", feature = "receive-msg"))]
            socket::ReceiveDatagrams::Single(datagram) => {
                #[cfg(feature = "prometheus")]
                metrics::counter!("lidi_receive_udp_packets").increment(1);

                let (datagram_session_id, packet) = protocol::session_split(datagram);

                track_session(&mut session_id, datagram_session_id, to_reblock)?;

                let packet = raptorq::EncodingPacket::deserialize(packet);

                #[cfg(not(feature = "receive-mmsg"))]
                to_reblock.send(reblock::Message::Packet(packet))?;
                #[cfg(feature = "receive-mmsg")]
                {
                    let mut packets = take_recycled(receiver);
                    packets.push(packet);
                    to_reblock.send(reblock::Message::Packets(packets))?;
                }
            }
            #[cfg(feature = "receive-mmsg")]
            socket::ReceiveDatagrams::Multiple(nb_msg, _) => {
                #[cfg(feature = "prometheus")]
                metrics::counter!("lidi_receive_udp_packets").increment(nb_msg as u64);

                // assume all datagrams are from the same session
                let datagram_session_id = protocol::session_split(udp.datagram(0)).0;

                track_session(&mut session_id, datagram_session_id, to_reblock)?;

                let mut packets = take_recycled(receiver);
                packets.extend((0..nb_msg).filter_map(|i| {
                    let (datagram_session_id, datagram) = protocol::session_split(udp.datagram(i));
                    if datagram_session_id == session_id {
                        Some(raptorq::EncodingPacket::deserialize(datagram))
                    } else {
                        None
                    }
                }));
                log::trace!(
                    "UDP recv: sending {} packets to reblock queue",
                    packets.len()
                );
                to_reblock.send(reblock::Message::Packets(packets))?;
            }
        }
    }
}
