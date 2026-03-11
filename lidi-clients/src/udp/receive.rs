#[cfg(feature = "tls")]
use crate::tls;
use crate::udp;
#[cfg(feature = "unix")]
use std::os::unix;
use std::{
    io::{Read, Write},
    net,
};

fn receive_udp<D>(
    config: &udp::Config<crate::DiodeReceive>,
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

#[cfg(feature = "tcp")]
fn receive_tcp_loop(
    config: &udp::Config<crate::DiodeReceive>,
    to_udp_bind: net::SocketAddr,
    to_udp: net::SocketAddr,
    server: &net::TcpListener,
) -> Result<(), udp::Error> {
    loop {
        let (client, client_addr) = server.accept()?;
        log::info!("new TCP client ({client_addr}) connected");
        match receive_udp(config, client, to_udp_bind, to_udp) {
            Ok(total) => log::info!("UDP received, {total} bytes received"),
            Err(e) => log::error!("failed to receive UDP: {e}"),
        }
    }
}

#[cfg(feature = "tls")]
fn receive_tls_loop(
    config: &udp::Config<crate::DiodeReceive>,
    to_udp_bind: net::SocketAddr,
    to_udp: net::SocketAddr,
    server: &tls::TcpListener,
) -> Result<(), udp::Error> {
    loop {
        let (client, client_addr) = server.accept()??;
        log::info!("new TLS client ({client_addr}) connected");
        match receive_udp(config, client, to_udp_bind, to_udp) {
            Ok(total) => log::info!("UDP received, {total} bytes received"),
            Err(e) => log::error!("failed to receive UDP: {e}"),
        }
    }
}

#[cfg(feature = "unix")]
fn receive_unix_loop(
    config: &udp::Config<crate::DiodeReceive>,
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
                .map_or_else(|| String::from("unknown"), |p| p.display().to_string())
        );
        match receive_udp(config, client, to_udp_bind, to_udp) {
            Ok(total) => log::info!("UDP received, {total} bytes received"),
            Err(e) => log::error!("failed to receive UDP: {e}"),
        }
    }
}

/// # Errors
///
/// Will return `Err` if `from_unix` `PathBuf`
/// already exists.
pub fn receive(
    config: &udp::Config<crate::DiodeReceive>,
    to_udp_bind: net::SocketAddr,
    to_udp: net::SocketAddr,
) -> Result<(), udp::Error> {
    #[cfg(feature = "tcp")]
    if let Some(from_tcp) = &config.diode.from_tcp {
        let server = net::TcpListener::bind(from_tcp)?;
        receive_tcp_loop(config, to_udp_bind, to_udp, &server)?;
    }

    #[cfg(feature = "tls")]
    if let Some(from_tls) = &config.diode.from_tls {
        let server = tls::TcpListener::bind(&config.tls, from_tls)?;
        receive_tls_loop(config, to_udp_bind, to_udp, &server)?;
    }

    #[cfg(feature = "unix")]
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

    Ok(())
}
