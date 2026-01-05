use crate::aux::{self, udp};
use std::{
    io::{Read, Write},
    net,
    os::unix,
};

fn send_udp_aux<D>(
    config: &udp::Config<aux::DiodeSend>,
    mut diode: D,
    from_udp: net::SocketAddr,
) -> Result<(), udp::Error>
where
    D: Read + Write,
{
    let mut buffer = vec![0; config.buffer_size];

    log::info!("binding UDP socket to {from_udp}");

    let socket = net::UdpSocket::bind(from_udp)?;

    loop {
        let (size, _) = socket.recv_from(&mut buffer)?;

        log::trace!("received datagram of {size} bytes");

        let header = udp::protocol::Header { size };
        header.serialize_to(&mut diode)?;
        diode.write_all(&buffer[..size])?;
    }
}

/// # Errors
///
/// Will return `Err` if:
/// - `net::TcpStream::connect(socket_addr)?`
///   or
/// - `unix::net::UnixStream::connect(path)?`
///   fails.
pub fn send(
    config: &udp::Config<aux::DiodeSend>,
    from_udp: net::SocketAddr,
) -> Result<(), udp::Error> {
    log::info!("connecting to {}", config.diode);

    match &config.diode {
        aux::DiodeSend::Tcp(socket_addr) => {
            let diode = net::TcpStream::connect(socket_addr)?;
            send_udp_aux(config, diode, from_udp)
        }
        aux::DiodeSend::Unix(path) => {
            let diode = unix::net::UnixStream::connect(path)?;
            send_udp_aux(config, diode, from_udp)
        }
    }
}
