use clap::{Arg, ArgAction, Command};
use diode::send;
use std::{
    env,
    io::Read,
    net,
    os::{fd::AsRawFd, unix},
    path,
    str::FromStr,
    thread, time,
};

struct Config {
    from_tcp: net::SocketAddr,
    from_unix: Option<path::PathBuf>,
    flush_timeout: Option<time::Duration>,
    nb_clients: u16,
    encoding_block_size: u64,
    repair_block_size: u32,
    nb_encoding_threads: u8,
    to_bind: net::SocketAddr,
    to_udp: net::SocketAddr,
    to_udp_mtu: u16,
    heartbeat: Option<time::Duration>,
}

fn command_args() -> Config {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_tcp")
                .long("from_tcp")
                .value_name("ip:port")
                .default_value("127.0.0.1:5000")
                .help("IP address and port to accept TCP clients"),
        )
        .arg(
            Arg::new("from_unix")
                .long("from_unix")
                .value_name("path")
                .help("Path of Unix socket to accept clients"),
        )
        .arg(
            Arg::new("flush_timeout")
                .long("flush_timeout")
                .value_name("nb_milliseconds")
                .default_value("1000")
                .value_parser(clap::value_parser!(u64))
                .help("Flush pending data after duration (0 = no flush)"),
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
            Arg::new("nb_encoding_threads")
                .long("nb_encoding_threads")
                .value_name("nb")
                .default_value("2")
                .value_parser(clap::value_parser!(u8))
                .help("Number of parallel RaptorQ encoding threads"),
        )
        .arg(
            Arg::new("encoding_block_size")
                .long("encoding_block_size")
                .value_name("nb_bytes")
                .default_value("60000") // (mtu * 40), optimal parameter -- to align with other size !
                .value_parser(clap::value_parser!(u64))
                .help("Size of RaptorQ block in bytes"),
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
            Arg::new("to_bind")
                .long("to_bind")
                .value_name("ip:port")
                .action(ArgAction::Set)
                .default_value("0.0.0.0:0")
                .help("Binding IP for UDP traffic"),
        )
        .arg(
            Arg::new("to_udp")
                .long("to_udp")
                .value_name("ip:port")
                .default_value("127.0.0.1:6000")
                .help("IP address and port where to send UDP packets to diode-receive"),
        )
        .arg(
            Arg::new("to_udp_mtu")
                .long("to_udp_mtu")
                .value_name("nb_bytes")
                .default_value("1500") // mtu
                .value_parser(clap::value_parser!(u16))
                .help("MTU of the output UDP link"),
        )
        .arg(
            Arg::new("heartbeat")
                .long("heartbeat")
                .value_name("nb_seconds")
                .default_value("5")
                .value_parser(clap::value_parser!(u16))
                .help("Duration between two emitted heartbeat messages, 0 to disable"),
        )
        .get_matches();

    let from_tcp = net::SocketAddr::from_str(args.get_one::<String>("from_tcp").expect("default"))
        .expect("invalid from_tcp parameter");
    let from_unix = args
        .get_one::<String>("from_unix")
        .map(|s| path::PathBuf::from_str(s).expect("invalid from_unix parameter"));
    let flush_timeout_ms = *args.get_one::<u64>("flush_timeout").expect("default");
    let flush_timeout = if flush_timeout_ms == 0 {
        None
    } else {
        Some(time::Duration::from_millis(flush_timeout_ms))
    };
    let nb_clients = *args.get_one::<u16>("nb_clients").expect("default");
    let nb_encoding_threads = *args.get_one::<u8>("nb_encoding_threads").expect("default");
    let encoding_block_size = *args.get_one::<u64>("encoding_block_size").expect("default");
    let repair_block_size = *args.get_one::<u32>("repair_block_size").expect("default");
    let to_bind = net::SocketAddr::from_str(args.get_one::<String>("to_bind").expect("default"))
        .expect("invalid to_bind parameter");
    let to_udp = net::SocketAddr::from_str(args.get_one::<String>("to_udp").expect("default"))
        .expect("invalid to_udp parameter");
    let to_udp_mtu = *args.get_one::<u16>("to_udp_mtu").expect("default");
    let heartbeat = {
        let hb = *args.get_one::<u16>("heartbeat").expect("default") as u64;
        (hb != 0).then(|| time::Duration::from_secs(hb))
    };

    Config {
        from_tcp,
        from_unix,
        flush_timeout,
        nb_clients,
        nb_encoding_threads,
        encoding_block_size,
        repair_block_size,
        to_bind,
        to_udp,
        to_udp_mtu,
        heartbeat,
    }
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

fn unix_listener_loop(
    listener: unix::net::UnixListener,
    sender: &send::Sender<Client>,
    timeout: Option<time::Duration>,
) {
    for client in listener.incoming() {
        match client {
            Err(e) => {
                log::error!("failed to accept client: {e}");
                return;
            }
            Ok(client) => {
                if let Err(e) = client.set_read_timeout(timeout) {
                    log::error!("failed to set client read timeout: {e}");
                }
                if let Err(e) = sender.new_client(Client::Unix(client)) {
                    log::error!("failed to send Unix client to connect queue: {e}");
                }
            }
        }
    }
}

fn tcp_listener_loop(
    listener: net::TcpListener,
    sender: &send::Sender<Client>,
    timeout: Option<time::Duration>,
) {
    for client in listener.incoming() {
        match client {
            Err(e) => {
                log::error!("failed to accept TCP client: {e}");
                return;
            }
            Ok(client) => {
                if let Err(e) = client.set_read_timeout(timeout) {
                    log::error!("failed to set client read timeout: {e}");
                }
                if let Err(e) = sender.new_client(Client::Tcp(client)) {
                    log::error!("failed to send TCP client to connect queue: {e}");
                }
            }
        }
    }
}

fn main() {
    let config = command_args();

    init_logger();

    let sender = send::Sender::new(send::Config {
        nb_clients: config.nb_clients,
        encoding_block_size: config.encoding_block_size,
        repair_block_size: config.repair_block_size,
        nb_encoding_threads: config.nb_encoding_threads,
        hearbeat_interval: config.heartbeat,
        to_bind: config.to_bind,
        to_udp: config.to_udp,
        to_mtu: config.to_udp_mtu,
    });

    thread::scope(|scope| {
        if let Err(e) = sender.start(scope) {
            log::error!("failed to start diode sender: {e}");
            return;
        }

        log::info!("accepting TCP clients at {}", config.from_tcp);

        let tcp_listener = match net::TcpListener::bind(config.from_tcp) {
            Err(e) => {
                log::error!("failed to bind TCP {}: {}", config.from_tcp, e);
                return;
            }
            Ok(listener) => listener,
        };

        thread::Builder::new()
            .name("diode-send-tcp-server".into())
            .spawn_scoped(scope, || {
                tcp_listener_loop(tcp_listener, &sender, config.flush_timeout)
            })
            .expect("thread spawn");

        if let Some(from_unix) = config.from_unix {
            if from_unix.exists() {
                log::error!("Unix socket path '{}' already exists", from_unix.display());
                return;
            }

            log::info!("accepting Unix clients at {}", from_unix.display());

            let unix_listener = match unix::net::UnixListener::bind(&from_unix) {
                Err(e) => {
                    log::error!("failed to bind Unix {}: {}", from_unix.display(), e);
                    return;
                }
                Ok(listener) => listener,
            };

            thread::Builder::new()
                .name("diode-send-unix-server".into())
                .spawn_scoped(scope, || {
                    unix_listener_loop(unix_listener, &sender, config.flush_timeout)
                })
                .expect("thread spawn");
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
