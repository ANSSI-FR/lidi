use lidi_command_utils::config;
#[cfg(feature = "to-tls")]
use lidi_command_utils::tls;
use lidi_protocol as protocol;
#[cfg(feature = "to-tcp")]
use std::net;
#[cfg(feature = "to-unix")]
use std::os::unix;
use std::{
    io::{self, Write},
    os::fd::AsRawFd,
    thread,
};

enum Client {
    #[cfg(feature = "to-tcp")]
    Tcp(net::TcpStream),
    #[cfg(feature = "to-tls")]
    Tls(tls::TcpStream),
    #[cfg(feature = "to-unix")]
    Unix(unix::net::UnixStream),
}

impl Write for Client {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        match self {
            #[cfg(feature = "to-tcp")]
            Self::Tcp(socket) => socket.write(buf),
            #[cfg(feature = "to-tls")]
            Self::Tls(socket) => socket.write(buf),
            #[cfg(feature = "to-unix")]
            Self::Unix(socket) => socket.write(buf),
        }
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        match self {
            #[cfg(feature = "to-tcp")]
            Self::Tcp(socket) => socket.flush(),
            #[cfg(feature = "to-tls")]
            Self::Tls(socket) => socket.flush(),
            #[cfg(feature = "to-unix")]
            Self::Unix(socket) => socket.flush(),
        }
    }
}

impl AsRawFd for Client {
    fn as_raw_fd(&self) -> i32 {
        match self {
            #[cfg(feature = "to-tcp")]
            Self::Tcp(socket) => socket.as_raw_fd(),
            #[cfg(feature = "to-tls")]
            Self::Tls(socket) => socket.as_raw_fd(),
            #[cfg(feature = "to-unix")]
            Self::Unix(socket) => socket.as_raw_fd(),
        }
    }
}

impl Client {
    fn new(endpoint: &lidi_command_utils::config::Endpoint) -> Result<Self, lidi_receive::Error> {
        match endpoint {
            lidi_command_utils::config::Endpoint::Tcp(to_tcp) => {
                #[cfg(not(feature = "to-tcp"))]
                {
                    let _ = to_tcp;
                    Err(lidi_receive::Error::Io(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "TCP endpoint not available (was not enabled at compilation)",
                    )))
                }
                #[cfg(feature = "to-tcp")]
                {
                    let client = net::TcpStream::connect(to_tcp)?;
                    Ok(Self::Tcp(client))
                }
            }
            lidi_command_utils::config::Endpoint::Tls(to_tls) => {
                let _ = to_tls;
                Err(lidi_receive::Error::Io(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "TLS endpoint not available (was not enabled at compilation)",
                )))
            }
            lidi_command_utils::config::Endpoint::Unix(to_unix) => {
                #[cfg(not(feature = "to-unix"))]
                {
                    let _ = to_unix;
                    Err(lidi_receive::Error::Io(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "Unix endpoint not available (was not enabled at compilation)",
                    )))
                }
                #[cfg(feature = "to-unix")]
                {
                    let client = unix::net::UnixStream::connect(to_unix)?;
                    Ok(Self::Unix(client))
                }
            }
        }
    }

    #[cfg(feature = "to-tls")]
    fn new_with_tls(
        tls: &lidi_command_utils::tls::ClientContext,
        endpoint: &lidi_command_utils::config::Endpoint,
    ) -> Result<Self, lidi_receive::Error> {
        match endpoint {
            lidi_command_utils::config::Endpoint::Tls(to_tls) => {
                let client = tls::TcpStream::connect(tls, to_tls)?;
                Ok(Self::Tls(client))
            }
            e => Self::new(e),
        }
    }
}

fn main() {
    let config = match lidi_command_utils::command_arguments(
        lidi_command_utils::Role::Receive,
        false,
        true,
        true,
    ) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let config = config::ReceiveConfig::from(config);

    let raptorq = match protocol::RaptorQ::new(
        config.common.mtu(),
        config.common.block(),
        config.common.repair(),
    ) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    #[cfg(feature = "to-tls")]
    let tls = match lidi_command_utils::tls::ClientContext::try_from(&config.receive.tls()) {
        Ok(tls) => tls,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let receiver = match lidi_receive::Receiver::new(
        &config,
        raptorq,
        |endpoint, _| {
            #[cfg(not(feature = "to-tls"))]
            {
                Client::new(endpoint)
            }
            #[cfg(feature = "to-tls")]
            {
                Client::new_with_tls(&tls, endpoint)
            }
        },
        |_, _| (),
    ) {
        Ok(receiver) => receiver,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    thread::scope(|scope| {
        if let Err(e) = receiver.start(scope) {
            log::error!("failed to start diode receiver: {e}");
        }
    });
}
