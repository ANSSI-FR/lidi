#[cfg(feature = "from-tls")]
use lidi_command_utils::tls;
use lidi_protocol as protocol;
use lidi_send as send;
#[cfg(any(feature = "from-tcp", feature = "from-tls"))]
use std::net;
use std::{
    io::{self, Read},
    os::fd::AsRawFd,
    sync, thread,
};
#[cfg(feature = "from-unix")]
use std::{os::unix, path};

enum Client {
    #[cfg(feature = "from-tcp")]
    Tcp(net::TcpStream),
    #[cfg(feature = "from-tls")]
    Tls(tls::TcpStream),
    #[cfg(feature = "from-unix")]
    Unix(unix::net::UnixStream),
}

impl Read for Client {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        match self {
            #[cfg(feature = "from-tcp")]
            Self::Tcp(stream) => stream.read(buf),
            #[cfg(feature = "from-tls")]
            Self::Tls(stream) => stream.read(buf),
            #[cfg(feature = "from-unix")]
            Self::Unix(stream) => stream.read(buf),
        }
    }
}

impl AsRawFd for Client {
    fn as_raw_fd(&self) -> i32 {
        match self {
            #[cfg(feature = "from-tcp")]
            Self::Tcp(stream) => stream.as_raw_fd(),
            #[cfg(feature = "from-tls")]
            Self::Tls(stream) => stream.as_raw_fd(),
            #[cfg(feature = "from-unix")]
            Self::Unix(stream) => stream.as_raw_fd(),
        }
    }
}

#[cfg(feature = "from-tcp")]
fn tcp_listener_loop(
    listener: &net::TcpListener,
    sender: &send::Sender<Client>,
    endpoint: protocol::EndpointId,
) {
    for client in listener.incoming() {
        match client {
            Err(e) => {
                log::error!("failed to accept TCP client: {e}");
                return;
            }
            Ok(stream) => {
                if let Err(e) = sender.new_client(endpoint, Client::Tcp(stream)) {
                    log::error!("failed to send TCP client to connect queue: {e}");
                }
            }
        }
    }
}

#[cfg(feature = "from-tcp")]
fn tcp_listener_start<'a>(
    scope: &'a thread::Scope<'a, '_>,
    sender: sync::Arc<send::Sender<Client>>,
    endpoint: protocol::EndpointId,
    from_tcp: &net::SocketAddr,
) -> Result<(), io::Error> {
    let listener = net::TcpListener::bind(from_tcp)?;

    log::info!("endpoint {endpoint} accepts TCP clients on {from_tcp}");

    thread::Builder::new()
        .name(format!("endpoint_{endpoint}"))
        .spawn_scoped(scope, move || {
            tcp_listener_loop(&listener, &sender, endpoint);
        })
        .expect("thread spawn");

    Ok(())
}

#[cfg(feature = "from-tls")]
fn tls_listener_loop(
    listener: &tls::TcpListener,
    sender: &send::Sender<Client>,
    endpoint: protocol::EndpointId,
) {
    loop {
        match listener.accept() {
            Err(e) => {
                log::error!("failed to accept TCP(TLS) client: {e}");
                return;
            }
            Ok(client) => match client {
                Err(e) => {
                    log::error!("failed to accept TLS client: {e}");
                }
                Ok(stream) => {
                    if let Err(e) = sender.new_client(endpoint, Client::Tls(stream)) {
                        log::error!("failed to send TLS client to connect queue: {e}");
                    }
                }
            },
        }
    }
}

#[cfg(feature = "from-tls")]
fn tls_listener_start<'a>(
    scope: &'a thread::Scope<'a, '_>,
    sender: sync::Arc<send::Sender<Client>>,
    endpoint: protocol::EndpointId,
    from_tls: &net::SocketAddr,
) -> Result<(), lidi_send::Error> {
    let listener = tls::TcpListener::bind(sender.tls(), from_tls)?;

    log::info!("endpoint {endpoint} accepts TLS clients on {from_tls}");

    thread::Builder::new()
        .name(format!("endpoint_{endpoint}"))
        .spawn_scoped(scope, move || {
            tls_listener_loop(&listener, &sender, endpoint);
        })
        .expect("thread spawn");

    Ok(())
}

#[cfg(feature = "from-unix")]
fn unix_listener_loop(
    listener: &unix::net::UnixListener,
    sender: &send::Sender<Client>,
    endpoint: protocol::EndpointId,
) {
    for client in listener.incoming() {
        match client {
            Err(e) => {
                log::error!("failed to accept client: {e}");
                return;
            }
            Ok(stream) => {
                if let Err(e) = sender.new_client(endpoint, Client::Unix(stream)) {
                    log::error!("failed to send Unix client to connect queue: {e}");
                }
            }
        }
    }
}

#[cfg(feature = "from-unix")]
fn unix_listener_start<'a>(
    scope: &'a thread::Scope<'a, '_>,
    sender: sync::Arc<send::Sender<Client>>,
    endpoint: protocol::EndpointId,
    from_unix: &path::PathBuf,
) -> Result<(), io::Error> {
    let listener = unix::net::UnixListener::bind(from_unix)?;

    log::info!(
        "endpoint {endpoint} accepts Unix clients on {}",
        from_unix.display()
    );

    thread::Builder::new()
        .name(format!("endpoint_{endpoint}"))
        .spawn_scoped(scope, move || {
            unix_listener_loop(&listener, &sender, endpoint);
        })
        .expect("thread spawn");

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn main() {
    let config =
        match lidi_command_utils::command_arguments(lidi_command_utils::Role::Send, false, true) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };

    let common = config.common();

    let raptorq = match protocol::RaptorQ::new(common.mtu(), common.block(), common.repair()) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let sender = match send::Sender::new(&config, raptorq) {
        Ok(sender) => sender,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let sender = sync::Arc::new(sender);

    thread::scope(|scope| {
        for (endpoint, from) in config.send().from().into_iter().enumerate() {
            let lsender = sender.clone();

            let endpoint = match u16::try_from(endpoint) {
                Ok(endpoint) => protocol::EndpointId::new(endpoint),
                Err(e) => {
                    log::error!("too many endpoints: {e}");
                    return;
                }
            };

            match from {
                lidi_command_utils::config::Endpoint::Tcp(from_tcp) => {
                    #[cfg(not(feature = "from-tcp"))]
                    {
                        let _ = from_tcp;
                        log::error!("TCP endpoint not available (was not enabled at compilation)");
                        return;
                    }
                    #[cfg(feature = "from-tcp")]
                    {
                        if let Err(e) = tcp_listener_start(scope, lsender, endpoint, &from_tcp) {
                            log::error!("failed to bind TCP {from_tcp}: {e}");
                            return;
                        }
                    }
                }
                lidi_command_utils::config::Endpoint::Tls(from_tls) => {
                    #[cfg(not(feature = "from-tls"))]
                    {
                        let _ = from_tls;
                        log::error!("TLS endpoint not available (was not enabled at compilation)");
                        return;
                    }
                    #[cfg(feature = "from-tls")]
                    {
                        if let Err(e) = tls_listener_start(scope, lsender, endpoint, &from_tls) {
                            log::error!("failed to bind TLS {from_tls}: {e}");
                            return;
                        }
                    }
                }
                lidi_command_utils::config::Endpoint::Unix(from_unix) => {
                    #[cfg(not(feature = "from-unix"))]
                    {
                        let _ = from_unix;
                        log::error!("Unix endpoint not available (was not enabled at compilation)");
                        return;
                    }
                    #[cfg(feature = "from-unix")]
                    {
                        if from_unix.exists() {
                            log::error!(
                                "Unix socket path '{}' already exists",
                                from_unix.display()
                            );
                            return;
                        }

                        if let Err(e) = unix_listener_start(scope, lsender, endpoint, &from_unix) {
                            log::error!("failed to bind Unix {}: {e}", from_unix.display());
                            return;
                        }
                    }
                }
            }
        }

        if let Err(e) = sender.start(scope) {
            log::error!("failed to start diode sender: {e}");
        }
    });
}
