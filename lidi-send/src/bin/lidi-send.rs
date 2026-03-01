use lidi_protocol as protocol;
use lidi_send as send;
#[cfg(feature = "endpoint-tcp")]
use std::net;
#[cfg(feature = "endpoint-unix")]
use std::os::unix;
use std::{
    io::{self, Read},
    os::fd::AsRawFd,
    sync, thread,
};

enum Client {
    #[cfg(feature = "endpoint-tcp")]
    Tcp(net::TcpStream),
    #[cfg(feature = "endpoint-unix")]
    Unix(unix::net::UnixStream),
}

impl Read for Client {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        match self {
            #[cfg(feature = "endpoint-tcp")]
            Self::Tcp(stream) => stream.read(buf),
            #[cfg(feature = "endpoint-unix")]
            Self::Unix(stream) => stream.read(buf),
        }
    }
}

impl AsRawFd for Client {
    fn as_raw_fd(&self) -> i32 {
        match self {
            #[cfg(feature = "endpoint-tcp")]
            Self::Tcp(stream) => stream.as_raw_fd(),
            #[cfg(feature = "endpoint-unix")]
            Self::Unix(stream) => stream.as_raw_fd(),
        }
    }
}

#[cfg(feature = "endpoint-unix")]
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

#[cfg(feature = "endpoint-tcp")]
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

fn main() {
    let config = match lidi_command_utils::command_arguments(lidi_command_utils::Role::Send, false)
    {
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
                    #[cfg(not(feature = "endpoint-tcp"))]
                    {
                        let _ = from_tcp;
                        log::error!("TP endpoint not available (was not enabled at compilation)");
                        return;
                    }
                    #[cfg(feature = "endpoint-tcp")]
                    {
                        match net::TcpListener::bind(from_tcp) {
                            Err(e) => {
                                log::error!("failed to bind TCP {from_tcp}: {e}");
                                return;
                            }
                            Ok(listener) => {
                                log::info!("endpoint {endpoint} accepts TCP clients on {from_tcp}");
                                thread::Builder::new()
                                    .name(format!("endpoint_{endpoint}"))
                                    .spawn_scoped(scope, move || {
                                        tcp_listener_loop(&listener, &lsender, endpoint);
                                    })
                                    .expect("thread spawn");
                            }
                        }
                    }
                }
                lidi_command_utils::config::Endpoint::Unix(from_unix) => {
                    #[cfg(not(feature = "endpoint-unix"))]
                    {
                        let _ = from_unix;
                        log::error!("Unix endpoint not available (was not enabled at compilation)");
                        return;
                    }
                    #[cfg(feature = "endpoint-unix")]
                    {
                        if from_unix.exists() {
                            log::error!(
                                "Unix socket path '{}' already exists",
                                from_unix.display()
                            );
                            return;
                        }

                        match unix::net::UnixListener::bind(&from_unix) {
                            Err(e) => {
                                log::error!("failed to bind Unix {}: {e}", from_unix.display());
                                return;
                            }
                            Ok(listener) => {
                                log::info!("accepting Unix clients at {}", from_unix.display());
                                thread::Builder::new()
                                    .name(format!("endpoint_{endpoint}"))
                                    .spawn_scoped(scope, move || {
                                        unix_listener_loop(&listener, &lsender, endpoint);
                                    })
                                    .expect("thread spawn");
                            }
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
