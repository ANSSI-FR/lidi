use crate::aux::{self, udp};
use std::{
    io::{Read, Write},
    net,
    os::unix,
};

fn receive_udp<D>(
    config: &udp::Config<aux::DiodeReceive>,
    mut diode: D,
    to_udp_bind: net::SocketAddr,
    to_udp: net::SocketAddr,
) -> Result<usize, udp::Error>
where
    D: Read + Write,
{
    log::debug!("binding UDP socket to {to_udp_bind}");

    let client = net::UdpSocket::bind(to_udp_bind)?;

    let mut buffer = vec![0; config.buffer_size];

    loop {
        let header = udp::protocol::Header::deserialize_from(&mut diode)?;

        log::trace!(
            "received header for datagram, reading {} bytes",
            header.size
        );

        diode.read_exact(&mut buffer[0..header.size])?;

        log::trace!("sending datagram to {to_udp}");

        client.send_to(&buffer[0..header.size], to_udp)?;
    }
}

fn receive_unix_loop(
    config: &udp::Config<aux::DiodeReceive>,
    to_udp_bind: net::SocketAddr,
    to_udp: net::SocketAddr,
    server: &unix::net::UnixListener,
) -> Result<(), udp::Error> {
    loop {
        let (client, client_addr) = server.accept()?;
        log::info!(
            "new Unix client ({}) connected",
            client_addr
                .as_pathname()
                .map_or("unknown".to_string(), |p| p.display().to_string())
        );
        match receive_udp(config, client, to_udp_bind, to_udp) {
            Ok(total) => log::info!("UDP received, {total} bytes received"),
            Err(e) => log::error!("failed to receive UDP: {e}"),
        }
    }
}

fn receive_tcp_loop(
    config: &udp::Config<aux::DiodeReceive>,
    to_udp_bind: net::SocketAddr,
    to_udp: net::SocketAddr,
    server: &net::TcpListener,
) -> Result<(), udp::Error> {
    loop {
        let (client, client_addr) = server.accept()?;
        log::info!("new Unix client ({client_addr}) connected");
        match receive_udp(config, client, to_udp_bind, to_udp) {
            Ok(total) => log::info!("UDP received, {total} bytes received"),
            Err(e) => log::error!("failed to receive UDP: {e}"),
        }
    }
}

pub fn receive(
    config: &udp::Config<aux::DiodeReceive>,
    to_udp_bind: net::SocketAddr,
    to_udp: net::SocketAddr,
) -> Result<(), udp::Error> {
    if let Some(from_unix) = &config.diode.from_unix {
        if from_unix.exists() {
            return Err(udp::Error::Other(format!(
                "Unix socket path '{}' already exists",
                from_unix.display()
            )));
        }

        let server = unix::net::UnixListener::bind(from_unix)?;
        receive_unix_loop(config, to_udp_bind, to_udp, &server)?;
    }

    if let Some(from_tcp) = &config.diode.from_tcp {
        let server = net::TcpListener::bind(from_tcp)?;
        receive_tcp_loop(config, to_udp_bind, to_udp, &server)?;
    }

    Ok(())
}
