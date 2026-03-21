use lidi_command_utils::config;
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
    endpoint_id: protocol::EndpointId,
    endpoint_options: config::EndpointOptions,
) {
    for client in listener.incoming() {
        match client {
            Err(e) => {
                log::error!("failed to accept TCP client: {e}");
                return;
            }
            Ok(stream) => {
                if let Err(e) =
                    sender.new_client(endpoint_id, endpoint_options, Client::Tcp(stream))
                {
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
    endpoint_id: protocol::EndpointId,
    endpoint_options: config::EndpointOptions,
    from_tcp: &net::SocketAddr,
) -> Result<(), io::Error> {
    let listener = net::TcpListener::bind(from_tcp)?;

    log::info!("endpoint {endpoint_id} accepts TCP clients on {from_tcp} ({endpoint_options})");
    #[cfg(not(feature = "hash"))]
    if endpoint_options.hash {
        log::warn!("hash was not enabled at compilation, ignoring this parameter");
    }

    thread::Builder::new()
        .name(format!("endpoint_{endpoint_id}"))
        .spawn_scoped(scope, move || {
            tcp_listener_loop(&listener, &sender, endpoint_id, endpoint_options);
        })
        .expect("thread spawn");

    Ok(())
}

#[cfg(feature = "from-tls")]
fn tls_listener_loop(
    listener: &tls::TcpListener,
    sender: &send::Sender<Client>,
    endpoint_id: protocol::EndpointId,
    endpoint_options: config::EndpointOptions,
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
                Ok((stream, client_addr)) => {
                    if let Err(e) =
                        sender.new_client(endpoint_id, endpoint_options, Client::Tls(stream))
                    {
                        log::error!(
                            "failed to send TLS client {client_addr} to connect queue: {e}"
                        );
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
    endpoint_id: protocol::EndpointId,
    endpoint_options: config::EndpointOptions,
    from_tls: &net::SocketAddr,
) -> Result<(), lidi_send::Error> {
    let listener = tls::TcpListener::bind(sender.tls(), from_tls)?;

    log::info!("endpoint {endpoint_id} accepts TLS clients on {from_tls} ({endpoint_options})");

    #[cfg(not(feature = "hash"))]
    if endpoint_options.hash {
        log::warn!("hash was not enabled at compilation, ignoring this parameter");
    }

    thread::Builder::new()
        .name(format!("endpoint_{endpoint_id}"))
        .spawn_scoped(scope, move || {
            tls_listener_loop(&listener, &sender, endpoint_id, endpoint_options);
        })
        .expect("thread spawn");

    Ok(())
}

#[cfg(feature = "from-unix")]
fn unix_listener_loop(
    listener: &unix::net::UnixListener,
    sender: &send::Sender<Client>,
    endpoint_id: protocol::EndpointId,
    endpoint_options: config::EndpointOptions,
) {
    for client in listener.incoming() {
        match client {
            Err(e) => {
                log::error!("failed to accept client: {e}");
                return;
            }
            Ok(stream) => {
                if let Err(e) =
                    sender.new_client(endpoint_id, endpoint_options, Client::Unix(stream))
                {
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
    endpoint_id: protocol::EndpointId,
    endpoint_options: config::EndpointOptions,
    from_unix: &path::PathBuf,
) -> Result<(), io::Error> {
    let listener = unix::net::UnixListener::bind(from_unix)?;

    log::info!(
        "endpoint {endpoint_id} accepts Unix clients on {} ({endpoint_options})",
        from_unix.display()
    );

    #[cfg(not(feature = "hash"))]
    if endpoint_options.hash {
        log::warn!("hash was not enabled at compilation, ignoring this parameter");
    }

    thread::Builder::new()
        .name(format!("endpoint_{endpoint_id}"))
        .spawn_scoped(scope, move || {
            unix_listener_loop(&listener, &sender, endpoint_id, endpoint_options);
        })
        .expect("thread spawn");

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn main() {
    let config = match lidi_command_utils::command_arguments(
        lidi_command_utils::Role::Send,
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

    let config = config::SendConfig::from(config);

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

    let sender = match send::Sender::new(&config, raptorq) {
        Ok(sender) => sender,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let sender = sync::Arc::new(sender);

    thread::scope(|scope| {
        for (endpoint_id, from) in config.send.from().into_iter().enumerate() {
            let lsender = sender.clone();

            let endpoint_id = match u16::try_from(endpoint_id) {
                Ok(endpoint) => protocol::EndpointId::new(endpoint),
                Err(e) => {
                    log::error!("too many endpoints: {e}");
                    return;
                }
            };

            match from {
                lidi_command_utils::config::Endpoint::Tcp { address, options } => {
                    #[cfg(not(feature = "from-tcp"))]
                    {
                        let _ = address;
                        let _ = options;
                        log::error!("TCP endpoint not available (was not enabled at compilation)");
                        return;
                    }
                    #[cfg(feature = "from-tcp")]
                    {
                        if let Err(e) =
                            tcp_listener_start(scope, lsender, endpoint_id, options, &address)
                        {
                            log::error!("failed to bind TCP {address}: {e}");
                            return;
                        }
                    }
                }
                lidi_command_utils::config::Endpoint::Tls { address, options } => {
                    #[cfg(not(feature = "from-tls"))]
                    {
                        let _ = address;
                        let _ = options;
                        log::error!("TLS endpoint not available (was not enabled at compilation)");
                        return;
                    }
                    #[cfg(feature = "from-tls")]
                    {
                        if let Err(e) =
                            tls_listener_start(scope, lsender, endpoint_id, options, &address)
                        {
                            log::error!("failed to bind TLS {address}: {e}");
                            return;
                        }
                    }
                }
                lidi_command_utils::config::Endpoint::Unix { path, options } => {
                    #[cfg(not(feature = "from-unix"))]
                    {
                        let _ = path;
                        let _ = options;
                        log::error!("Unix endpoint not available (was not enabled at compilation)");
                        return;
                    }
                    #[cfg(feature = "from-unix")]
                    {
                        if path.exists() {
                            log::error!("Unix socket path '{}' already exists", path.display());
                            return;
                        }

                        if let Err(e) =
                            unix_listener_start(scope, lsender, endpoint_id, options, &path)
                        {
                            log::error!("failed to bind Unix {}: {e}", path.display());
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
