use clap::Parser;
use diode::{protocol, receive};
use std::{
    io::{self, Write},
    net,
    os::{fd::AsRawFd, unix},
    path,
    str::FromStr,
    thread, time,
};

fn parse_duration_seconds(input: &str) -> Result<time::Duration, <u64 as FromStr>::Err> {
    let input = input.parse()?;
    Ok(time::Duration::from_secs(input))
}

#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct Clients {
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port to connect to TCP server"
    )]
    to_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path of socket to connect to Unix server"
    )]
    to_unix: Option<path::PathBuf>,
}

#[derive(Parser)]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port where to receive UDP packets from diode-send"
    )]
    from: net::SocketAddr,
    #[clap(
        default_value = "1500",
        value_name = "nb_bytes",
        long,
        help = "MTU of the input UDP link"
    )]
    from_mtu: u16,
    #[clap(
        value_name = "2..1024",
        long,
        help = "Use recvmmsg to receive from 2 to 1024 UDP datagrams at once"
    )]
    batch: Option<u32>,
    #[clap(
        default_value = "2",
        value_name = "seconds",
        value_parser = parse_duration_seconds,
        long,
        help = "Reset diode if no data are received after duration")]
    reset_timeout: time::Duration,
    #[clap(
        default_value = "1",
        value_name = "0..255",
        long,
        help = "Number of parallel RaptorQ decode threads"
    )]
    decode_threads: u8,
    #[clap(
        default_value = "2",
        value_name = "clients",
        long,
        help = "Max number of simultaneous clients/transfers"
    )]
    max_clients: protocol::ClientId,
    #[clap(long, help = "Flush immediately data to clients")]
    flush: bool,
    #[clap(
        value_name = "seconds",
        value_parser = parse_duration_seconds,
        long,
        help = "Abort connections if no data received after duration (0 = no abort)")]
    abort_timeout: Option<time::Duration>,
    #[clap(flatten)]
    to: Clients,
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
    #[clap(
        default_value = "10",
        value_name = "nb_seconds",
        value_parser = parse_duration_seconds,
        long,
        help = "Maximum duration expected between heartbeat messages, 0 to disable")]
    heartbeat: Option<time::Duration>,
    #[clap(long, help = "Set CPU affinity for threads")]
    cpu_affinity: bool,
}

enum Client {
    Tcp(net::TcpStream),
    Unix(unix::net::UnixStream),
}

impl Write for Client {
    fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        match self {
            Self::Tcp(socket) => socket.write(buf),
            Self::Unix(socket) => socket.write(buf),
        }
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        match self {
            Self::Tcp(socket) => socket.flush(),
            Self::Unix(socket) => socket.flush(),
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

impl TryFrom<&Clients> for Client {
    type Error = io::Error;

    fn try_from(clients: &Clients) -> Result<Self, Self::Error> {
        if let Some(to_tcp) = clients.to_tcp.as_ref() {
            let client = net::TcpStream::connect(to_tcp)?;
            Ok(Self::Tcp(client))
        } else if let Some(to_unix) = clients.to_unix.as_ref() {
            let client = unix::net::UnixStream::connect(to_unix)?;
            Ok(Self::Unix(client))
        } else {
            unreachable!()
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

    let raptorq = match protocol::RaptorQ::new(args.from_mtu, args.block, args.repair) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let receiver = match receive::Receiver::new(
        receive::Config {
            from: args.from,
            from_mtu: args.from_mtu,
            max_clients: args.max_clients,
            flush: args.flush,
            reset_timeout: args.reset_timeout,
            nb_decode_threads: args.decode_threads,
            abort_timeout: args.abort_timeout,
            heartbeat_interval: args.heartbeat,
            batch_receive: args.batch,
            cpu_affinity: args.cpu_affinity,
        },
        raptorq,
        || Client::try_from(&args.to),
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
