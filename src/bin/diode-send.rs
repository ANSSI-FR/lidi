use clap::Parser;
use diode::{protocol, send};
use std::{
    io::Read,
    net,
    os::{fd::AsRawFd, unix},
    path,
    str::FromStr,
    sync, thread, time,
};

fn parse_duration_seconds(input: &str) -> Result<time::Duration, <u64 as FromStr>::Err> {
    let input = input.parse()?;
    Ok(time::Duration::from_secs(input))
}

#[derive(clap::Args)]
#[group(required = true, multiple = true)]
struct Listeners {
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port to accept TCP clients"
    )]
    from_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path of Unix socket to accept clients"
    )]
    from_unix: Option<path::PathBuf>,
}

#[derive(clap::Parser)]
#[clap(long_about = None)]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(flatten)]
    from: Listeners,
    #[clap(
        default_value = "2",
        value_name = "nb_clients",
        long,
        help = "Max number of simultaneous clients/transfers"
    )]
    max_clients: protocol::ClientId,
    #[clap(
        default_value = "1",
        value_name = "0..255",
        long,
        help = "Number of parallel RaptorQ encoding threads"
    )]
    encode_threads: u8,
    #[clap(
        default_value = "5",
        value_name = "nb_seconds",
        value_parser = parse_duration_seconds,
        long,
        help = "Duration between two emitted heartbeat messages, 0 to disable"
    )]
    heartbeat: Option<time::Duration>,
    #[clap(long, help = "Flush client data immediately")]
    flush: bool,
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port where to send UDP packets to diode-receive"
    )]
    to: net::SocketAddr,
    #[clap(
        default_value = "0.0.0.0:0",
        value_name = "ip:port",
        long,
        help = "Binding IP for UDP traffic"
    )]
    to_bind: net::SocketAddr,
    #[clap(
        default_value = "1500",
        value_name = "nb_bytes",
        long,
        help = "MTU of the output UDP link"
    )]
    to_mtu: u16,
    #[clap(
        value_name = "2..1024",
        long,
        help = "Use sendmmsg to send from 2 to 1024 UDP datagrams at once"
    )]
    batch: Option<u32>,
    #[clap(
        default_value = "734928",
        value_name = "nb_bytes",
        long,
        help = "Size of RaptorQ block in bytes"
    )]
    block: u32,
    #[clap(
        default_value = "2",
        value_name = "percentage",
        long,
        help = "Percentage of RaptorQ repair data"
    )]
    repair: u32,
    #[clap(long, help = "Set CPU affinity for threads")]
    cpu_affinity: bool,
}

enum Client {
    Tcp(net::TcpStream),
    Unix(unix::net::UnixStream),
}

impl Read for Client {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        match self {
            Self::Tcp(socket) => socket.read(buf),
            Self::Unix(socket) => socket.read(buf),
        }
    }
}

impl AsRawFd for Client {
    fn as_raw_fd(&self) -> i32 {
        match self {
            Self::Tcp(socket) => socket.as_raw_fd(),
            Self::Unix(socket) => socket.as_raw_fd(),
        }
    }
}

fn unix_listener_loop(listener: &unix::net::UnixListener, sender: &send::Sender<Client>) {
    for client in listener.incoming() {
        match client {
            Err(e) => {
                log::error!("failed to accept client: {e}");
                return;
            }
            Ok(client) => {
                if let Err(e) = sender.new_client(Client::Unix(client)) {
                    log::error!("failed to send Unix client to connect queue: {e}");
                }
            }
        }
    }
}

fn tcp_listener_loop(listener: &net::TcpListener, sender: &send::Sender<Client>) {
    for client in listener.incoming() {
        match client {
            Err(e) => {
                log::error!("failed to accept TCP client: {e}");
                return;
            }
            Ok(client) => {
                if let Err(e) = sender.new_client(Client::Tcp(client)) {
                    log::error!("failed to send TCP client to connect queue: {e}");
                }
            }
        }
    }
}

fn main() {
    let args = Args::parse();

    diode::init_logger(args.log_level);

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let raptorq = match protocol::RaptorQ::new(args.to_mtu, args.block, args.repair) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let sender = match send::Sender::new(
        send::Config {
            max_clients: args.max_clients,
            flush: args.flush,
            nb_encode_threads: args.encode_threads,
            heartbeat_interval: args.heartbeat,
            to: args.to,
            to_bind: args.to_bind,
            to_mtu: args.to_mtu,
            batch_send: args.batch,
            cpu_affinity: args.cpu_affinity,
        },
        raptorq,
    ) {
        Ok(sender) => sender,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let tcp_listener = match args.from.from_tcp {
        None => None,
        Some(from_tcp) => match net::TcpListener::bind(from_tcp) {
            Err(e) => {
                log::error!("failed to bind TCP {from_tcp}: {e}");
                return;
            }
            Ok(listener) => {
                log::info!("accepting TCP clients on {from_tcp}");
                Some(listener)
            }
        },
    };

    let unix_listener = match args.from.from_unix {
        None => None,
        Some(from_unix) => {
            if from_unix.exists() {
                log::error!("Unix socket path '{}' already exists", from_unix.display());
                return;
            }

            match unix::net::UnixListener::bind(&from_unix) {
                Err(e) => {
                    log::error!("failed to bind Unix {}: {e}", from_unix.display());
                    return;
                }
                Ok(listener) => {
                    log::info!("accepting Unix clients at {}", from_unix.display());
                    Some(listener)
                }
            }
        }
    };

    let sender = sync::Arc::new(sender);

    thread::scope(|scope| {
        let lsender = sender.clone();
        if let Some(tcp_listener) = tcp_listener {
            thread::Builder::new()
                .name("tcp_server".into())
                .spawn_scoped(scope, move || tcp_listener_loop(&tcp_listener, &lsender))
                .expect("thread spawn");
        }

        let lsender = sender.clone();
        if let Some(unix_listener) = unix_listener {
            thread::Builder::new()
                .name("unix_server".into())
                .spawn_scoped(scope, move || unix_listener_loop(&unix_listener, &lsender))
                .expect("thread spawn");
        }

        if let Err(e) = sender.start(scope) {
            log::error!("failed to start diode sender: {e}");
        }
    });
}
