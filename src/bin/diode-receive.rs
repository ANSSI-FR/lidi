use clap::{Arg, ArgGroup, Command};
use diode::receive;
use std::{
    env, fmt,
    io::{self, Write},
    net,
    num::NonZeroU64,
    os::{fd::AsRawFd, unix},
    path,
    str::FromStr,
    thread, time,
};

struct Config {
    from_udp: net::SocketAddr,
    from_udp_mtu: u16,
    nb_clients: u16,
    encoding_block_size: u64,
    repair_block_size: u32,
    udp_buffer_size: u32,
    reblock_retention_window: u8,
    flush_timeout: time::Duration,
    nb_decoding_threads: u8,
    to: ClientConfig,
    heartbeat: Option<time::Duration>,
}

enum ClientConfig {
    Tcp(net::SocketAddr),
    Unix(path::PathBuf),
}

impl fmt::Display for ClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Tcp(s) => write!(f, "TCP {s}"),
            Self::Unix(p) => write!(f, "Unix {}", p.display()),
        }
    }
}

fn command_args() -> Config {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_udp")
                .long("from_udp")
                .value_name("ip:port")
                .default_value("127.0.0.1:6000")
                .help("IP address and port where to receive UDP packets from diode-send"),
        )
        .arg(
            Arg::new("from_udp_mtu")
                .long("from_udp_mtu")
                .value_name("nb_bytes")
                .default_value("1500") // mtu
                .value_parser(clap::value_parser!(u16))
                .help("MTU of the input UDP link"),
        )
        .arg(
            Arg::new("nb_clients")
                .long("nb_clients")
                .value_name("nb")
                .default_value("2")
                .value_parser(clap::value_parser!(u16))
                .help("Number of simultaneous transfers"),
        )
        .arg(
            Arg::new("nb_decoding_threads")
                .long("nb_decoding_threads")
                .value_name("nb")
                .default_value("1")
                .value_parser(clap::value_parser!(u8))
                .help("Number of parallel RaptorQ decoding threads"),
        )
        .arg(
            Arg::new("encoding_block_size")
                .long("encoding_block_size")
                .value_name("nb_bytes")
                .default_value("60000") // (mtu * 40), optimal parameter -- to align with other size !
                .value_parser(clap::value_parser!(u64))
                .help("Size of RaptorQ block"),
        )
        .arg(
            Arg::new("repair_block_size")
                .long("repair_block_size")
                .value_name("ratior")
                .default_value("6000") // mtu * 4
                .value_parser(clap::value_parser!(u32))
                .help("Size of repair data in bytes"),
        )
        .arg(
            Arg::new("udp_buffer_size")
                .long("udp_buffer_size")
                .value_name("nb_bytes")
                .default_value("1073741823") // i32::MAX / 2
                .value_parser(clap::value_parser!(u32).range(..1073741824))
                .help("Size of UDP socket recv buffer"),
        )
        .arg(
            Arg::new("reblock_retention_window")
                .long("reblock_retention_window")
                .value_name("reblock_retention_window")
                .default_value("8")
                .value_parser(clap::value_parser!(u8).range(1..128))
                .help("Higher value increases resilience to blocks getting mixed up"),
        )
        .arg(
            Arg::new("flush_timeout")
                .long("flush_timeout")
                .value_name("nb_milliseconds")
                .default_value("1000")
                .value_parser(clap::value_parser!(NonZeroU64))
                .help("Flush pending data after duration"),
        )
        .arg(
            Arg::new("to_tcp")
                .long("to_tcp")
                .value_name("ip:port")
                .help("IP address and port to connect to TCP server"),
        )
        .arg(
            Arg::new("to_unix")
                .long("to_unix")
                .value_name("path")
                .help("Path of socket to connect to Unix server"),
        )
        .group(
            ArgGroup::new("to")
                .required(true)
                .args(["to_tcp", "to_unix"]),
        )
        .arg(
            Arg::new("heartbeat")
                .long("heartbeat")
                .value_name("nb_seconds")
                .default_value("10")
                .value_parser(clap::value_parser!(u16))
                .help("Maximum duration expected between heartbeat messages, 0 to disable"),
        )
        .get_matches();

    let from_udp = net::SocketAddr::from_str(args.get_one::<String>("from_udp").expect("default"))
        .expect("invalid from_udp parameter");
    let from_udp_mtu = *args.get_one::<u16>("from_udp_mtu").expect("default");
    let nb_clients = *args.get_one::<u16>("nb_clients").expect("default");
    let nb_decoding_threads = *args.get_one::<u8>("nb_decoding_threads").expect("default");
    let encoding_block_size = *args.get_one::<u64>("encoding_block_size").expect("default");
    let udp_buffer_size = *args.get_one::<u32>("udp_buffer_size").expect("default");
    let reblock_retention_window = *args
        .get_one::<u8>("reblock_retention_window")
        .expect("default");
    let repair_block_size = *args.get_one::<u32>("repair_block_size").expect("default");
    let flush_timeout = time::Duration::from_millis(
        args.get_one::<NonZeroU64>("flush_timeout")
            .expect("default")
            .get(),
    );
    let to_tcp = args
        .get_one::<String>("to_tcp")
        .map(|s| net::SocketAddr::from_str(s).expect("to_tcp must be of the form ip:port"));
    let to_unix = args
        .get_one::<String>("to_unix")
        .map(|s| path::PathBuf::from_str(s).expect("to_unix must point to a valid path"));

    let heartbeat = {
        let hb = *args.get_one::<u16>("heartbeat").expect("default") as u64;
        (hb != 0).then(|| time::Duration::from_secs(hb))
    };

    let to = if let Some(to_tcp) = to_tcp {
        ClientConfig::Tcp(to_tcp)
    } else {
        ClientConfig::Unix(to_unix.expect("to_tcp and to_unix are mutually exclusive"))
    };

    Config {
        from_udp,
        from_udp_mtu,
        nb_clients,
        nb_decoding_threads,
        encoding_block_size,
        repair_block_size,
        udp_buffer_size,
        reblock_retention_window,
        flush_timeout,
        to,
        heartbeat,
    }
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

impl TryFrom<&ClientConfig> for Client {
    type Error = io::Error;

    fn try_from(config: &ClientConfig) -> Result<Self, Self::Error> {
        match config {
            ClientConfig::Tcp(s) => {
                let client = net::TcpStream::connect(s)?;
                Ok(Self::Tcp(client))
            }
            ClientConfig::Unix(p) => {
                let client = unix::net::UnixStream::connect(p)?;
                Ok(Self::Unix(client))
            }
        }
    }
}

fn main() {
    let config = command_args();

    init_logger();

    log::info!("sending traffic to {}", config.to);

    let receiver = receive::Receiver::new(
        receive::Config {
            from_udp: config.from_udp,
            from_udp_mtu: config.from_udp_mtu,
            nb_clients: config.nb_clients,
            encoding_block_size: config.encoding_block_size,
            repair_block_size: config.repair_block_size,
            udp_buffer_size: config.udp_buffer_size,
            reblock_retention_window: config.reblock_retention_window,
            flush_timeout: config.flush_timeout,
            nb_decoding_threads: config.nb_decoding_threads,
            heartbeat_interval: config.heartbeat,
        },
        || Client::try_from(&config.to),
    );

    thread::scope(|scope| {
        if let Err(e) = receiver.start(scope) {
            log::error!("failed to start diode receiver: {e}");
        }
    });
}

fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env()
    } else {
        simple_logger::init_with_level(log::Level::Info)
    }
    .expect("logger initialization")
}
