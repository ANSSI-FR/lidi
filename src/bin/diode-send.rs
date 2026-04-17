use clap::Parser;
use diode::{http, protocol, send, stats};
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
#[clap(about = "Sender part of lidi.")]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Off|Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(
        value_name = "path",
        long,
        help = "Log messages in a file instead of the console"
    )]
    log_file: Option<path::PathBuf>,
    #[clap(
        value_name = "ip:port",
        long,
        help = "Bind a read-only observability HTTP server on this address (bind to 127.0.0.1; no auth)"
    )]
    http_addr: Option<net::SocketAddr>,
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
    #[clap(long, help = "Hash each client transfered data")]
    hash: bool,
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

fn bind_tcp(from_tcp: Option<net::SocketAddr>) -> Result<Option<net::TcpListener>, ()> {
    let Some(addr) = from_tcp else {
        return Ok(None);
    };
    match net::TcpListener::bind(addr) {
        Err(e) => {
            log::error!("failed to bind TCP {addr}: {e}");
            Err(())
        }
        Ok(listener) => {
            log::info!("accepting TCP clients on {addr}");
            Ok(Some(listener))
        }
    }
}

fn bind_unix(from_unix: Option<path::PathBuf>) -> Result<Option<unix::net::UnixListener>, ()> {
    let Some(path) = from_unix else {
        return Ok(None);
    };
    if path.exists() {
        log::error!("Unix socket path '{}' already exists", path.display());
        return Err(());
    }
    match unix::net::UnixListener::bind(&path) {
        Err(e) => {
            log::error!("failed to bind Unix {}: {e}", path.display());
            Err(())
        }
        Ok(listener) => {
            log::info!("accepting Unix clients at {}", path.display());
            Ok(Some(listener))
        }
    }
}

fn spawn_http(
    addr: net::SocketAddr,
    stats: sync::Arc<diode::stats::Stats>,
    info: sync::Arc<http::InfoSnapshot>,
    log_ring: Option<sync::Arc<diode::logring::LogRing>>,
) {
    // Detached: the HTTP server outlives scoped-thread shutdown (the OS reaps
    // it when `main` returns).
    thread::Builder::new()
        .name("http_server".into())
        .spawn(move || http::start(addr, &stats, &info, log_ring.as_ref()))
        .expect("thread spawn");
}

fn build_info(args: &Args) -> http::InfoSnapshot {
    http::InfoSnapshot {
        role: stats::ROLE_SEND,
        version: env!("CARGO_PKG_VERSION"),
        max_clients: args.max_clients,
        block_bytes: args.block,
        repair_pct: args.repair,
        heartbeat_secs: args.heartbeat.map(|d| d.as_secs()),
        mtu: args.to_mtu,
        peer: Some(args.to),
        bind: Some(args.to_bind),
        listener_tcp: args.from.from_tcp,
        listener_unix: args.from.from_unix.clone(),
        forward_tcp: None,
        forward_unix: None,
        flush: args.flush,
        hash: args.hash,
    }
}

fn main() {
    let args = Args::parse();

    let log_ring = match diode::init_logger_with_ring(
        args.log_level,
        args.log_file.clone(),
        false,
        args.http_addr.is_some(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("failed to initialize logger: {e}");
            return;
        }
    };

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let raptorq = match protocol::RaptorQ::new(args.to_mtu, args.block, args.repair, 0) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let sender_stats = sync::Arc::new(stats::Stats::new(
        stats::ROLE_SEND,
        (args.max_clients as usize).saturating_mul(8).max(32),
    ));
    let info = sync::Arc::new(build_info(&args));

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
            hash: args.hash,
        },
        raptorq,
        sync::Arc::clone(&sender_stats),
    ) {
        Ok(sender) => sender,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let Ok(tcp_listener) = bind_tcp(args.from.from_tcp) else {
        return;
    };
    let Ok(unix_listener) = bind_unix(args.from.from_unix.clone()) else {
        return;
    };

    let sender = sync::Arc::new(sender);

    if let Some(http_addr) = args.http_addr {
        spawn_http(
            http_addr,
            sync::Arc::clone(&sender_stats),
            sync::Arc::clone(&info),
            log_ring,
        );
    }

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
