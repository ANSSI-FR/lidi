#[cfg(feature = "tls")]
use crate::tls;
use crate::udp;
#[cfg(feature = "unix")]
use std::os::unix;
use std::{
    io::{Read, Write},
    net,
};

fn send_udp_aux<D>(
    config: &udp::Config<crate::DiodeSend>,
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
    config: &udp::Config<crate::DiodeSend>,
    from_udp: net::SocketAddr,
) -> Result<(), udp::Error> {
    log::info!("connecting to {}", config.diode);

    match &config.diode {
        crate::DiodeSend::Tcp(socket_addr) => {
            #[cfg(not(feature = "tcp"))]
            {
                let _ = socket_addr;
                log::error!("TCP was not enable at compilation");
                Ok(())
            }
            #[cfg(feature = "tcp")]
            {
                let diode = net::TcpStream::connect(socket_addr)?;
                send_udp_aux(config, diode, from_udp)
            }
        }
        crate::DiodeSend::Tls(socket_addr) => {
            #[cfg(not(feature = "tls"))]
            {
                let _ = socket_addr;
                log::error!("TLS was not enable at compilation");
                Ok(())
            }
            #[cfg(feature = "tls")]
            {
                let context = tls::ClientContext::try_from(&config.tls)?;
                let diode = tls::TcpStream::connect(&context, socket_addr)?;
                send_udp_aux(config, diode, from_udp)
            }
        }
        crate::DiodeSend::Unix(path) => {
            #[cfg(not(feature = "unix"))]
            {
                let _ = path;
                log::error!("Unix was not enable at compilation");
                Ok(())
            }
            #[cfg(feature = "unix")]
            {
                let diode = unix::net::UnixStream::connect(path)?;
                send_udp_aux(config, diode, from_udp)
            }
        }
    }
}
