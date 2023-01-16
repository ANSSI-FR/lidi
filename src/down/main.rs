mod encoding;
mod tcp_client;
mod udp_send;

use clap::{Arg, ArgAction, Command};
use crossbeam_channel::{bounded, unbounded, Receiver, RecvError, Sender};
use log::{debug, error, info};
use std::{
    fmt,
    net::{SocketAddr, TcpListener, TcpStream},
    str::FromStr,
    thread,
};

struct Config {
    from_tcp: SocketAddr,
    from_tcp_buffer_size: usize,

    nb_transfers: u16,

    encoding_block_size: u64,
    repair_block_size: u32,
    flush_timeout: u64,

    to_udp: SocketAddr,
    to_udp_mtu: u16,
}

impl Default for Config {
    fn default() -> Self {
        let mtu = 1500;
        Self {
            from_tcp: SocketAddr::from_str("127.0.0.1:5000").unwrap(),
            from_tcp_buffer_size: mtu * 10,

            nb_transfers: 2,

            encoding_block_size: (mtu * 20) as u64, //optimal parameter -- to align with other size !
            repair_block_size: (mtu * 2) as u32,
            flush_timeout: 2,

            to_udp: SocketAddr::from_str("127.0.0.1:6000").unwrap(),
            to_udp_mtu: mtu as u16,
        }
    }
}

enum Error {
    Crossbeam(RecvError),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Crossbeam(e) => write!(fmt, "crossbeam error: {e}"),
        }
    }
}

impl From<RecvError> for Error {
    fn from(e: RecvError) -> Self {
        Self::Crossbeam(e)
    }
}

fn connect_loop_aux(
    connect_recvq: Receiver<TcpStream>,
    tcp_client_config: tcp_client::Config,
    tcp_sendq: Sender<diode::ClientMessage>,
) -> Result<(), Error> {
    loop {
        let client = connect_recvq.recv()?;
        tcp_client::new(&tcp_client_config, client, tcp_sendq.clone());
    }
}

fn connect_loop(
    connect_recvq: Receiver<TcpStream>,
    tcp_client_config: tcp_client::Config,
    tcp_senq: Sender<diode::ClientMessage>,
) {
    if let Err(e) = connect_loop_aux(connect_recvq, tcp_client_config, tcp_senq) {
        error!("failed to connect client: {e}");
    }
}

fn command_args(config: &mut Config) {
    let args = Command::new("diode-down")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_tcp")
                .long("from_tcp")
                .action(ArgAction::Set)
                .value_name("ip:port")
                .help("From where to read data"),
        )
        .arg(
            Arg::new("from_tcp_buffer_size")
                .long("from_tcp_buffer_size")
                .action(ArgAction::Set)
                .value_name("nb_bytes")
                .value_parser(clap::value_parser!(usize))
                .help("Size of TCP read buffer"),
        )
        .arg(
            Arg::new("nb_transfers")
                .long("nb_transfers")
                .action(ArgAction::Set)
                .value_name("nb")
                .value_parser(clap::value_parser!(u16))
                .help("Number of simultaneous transfers"),
        )
        .arg(
            Arg::new("encoding_block_size")
                .long("encoding_block_size")
                .action(ArgAction::Set)
                .value_name("nb_bytes")
                .value_parser(clap::value_parser!(u64))
                .help("Size of RaptorQ block in bytes"),
        )
        .arg(
            Arg::new("repair_block_size")
                .long("repair_block_size")
                .action(ArgAction::Set)
                .value_name("ratior")
                .value_parser(clap::value_parser!(u32))
                .help("Size of repair data in bytes"),
        )
        .arg(
            Arg::new("flush_timeout")
                .long("flush_timeout")
                .action(ArgAction::Set)
                .value_name("nb_seconds")
                .value_parser(clap::value_parser!(u64))
                .help("Duration in seconds after an incomplete RaptorQ block is flushed"),
        )
        .arg(
            Arg::new("to_udp")
                .long("to_udp")
                .action(ArgAction::Set)
                .value_name("ip:port")
                .help("Where to send data"),
        )
        .arg(
            Arg::new("to_udp_mtu")
                .long("to_udp_mtu")
                .action(ArgAction::Set)
                .value_name("nb_bytes")
                .value_parser(clap::value_parser!(u16))
                .help("MTU in bytes of output UDP link"),
        )
        .get_matches();

    if let Some(p) = args.get_one::<String>("from_tcp") {
        let p = SocketAddr::from_str(p).expect("invalid from_tcp parameter");
        config.from_tcp = p;
    }

    if let Some(p) = args.get_one::<usize>("from_tcp_buffer_size") {
        config.from_tcp_buffer_size = *p;
    }

    if let Some(p) = args.get_one::<u16>("nb_transfers") {
        config.nb_transfers = *p;
    }

    if let Some(p) = args.get_one::<u64>("encoding_block_size") {
        config.encoding_block_size = *p;
    }

    if let Some(p) = args.get_one::<u32>("repair_block_size") {
        config.repair_block_size = *p;
    }

    if let Some(p) = args.get_one::<u64>("flush_timeout") {
        config.flush_timeout = *p;
    }

    if let Some(p) = args.get_one::<String>("to_udp") {
        let p = SocketAddr::from_str(p).expect("invalid to_udp parameter");
        config.to_udp = p;
    }

    if let Some(p) = args.get_one::<u16>("to_udp_mtu") {
        config.to_udp_mtu = *p;
    }
}

fn main() {
    let mut config = Config::default();

    command_args(&mut config);

    diode::init_logger();

    config.encoding_block_size =
        diode::adjust_encoding_block_size(config.to_udp_mtu, config.encoding_block_size);

    debug!(
        "adjusting encoding_block_size to {} bytes",
        config.encoding_block_size
    );

    info!(
        "accepting TCP clients at {} with read buffer of {} bytes",
        config.from_tcp, config.from_tcp_buffer_size
    );

    let tcp_client_config = tcp_client::Config {
        buffer_size: config.from_tcp_buffer_size,
    };

    let encoding_config = encoding::Config {
        logical_block_size: config.encoding_block_size,
        repair_block_size: config.repair_block_size,
        output_mtu: config.to_udp_mtu,
        flush_timeout: config.flush_timeout,
    };

    info!(
        "encoding with block size of {} bytes and repair block size of {} bytes and a flush timeout of {} second(s)",
        encoding_config.logical_block_size,
        encoding_config.repair_block_size,
        encoding_config.flush_timeout,
    );

    let udp_send_config = udp_send::Config {
        to_udp: config.to_udp,
        mtu: config.to_udp_mtu,
    };

    info!(
        "sending UDP traffic to {} with MTU {}",
        udp_send_config.to_udp, udp_send_config.mtu
    );

    let (connect_sendq, connect_recvq) = bounded::<TcpStream>(1);
    let (tcp_sendq, tcp_recvq) = bounded::<diode::ClientMessage>(config.nb_transfers as usize);
    let (udp_sendq, udp_recvq) = unbounded::<udp_send::Message>();

    thread::spawn(move || udp_send::new(udp_send_config, udp_recvq));

    thread::spawn(move || encoding::new(encoding_config, tcp_recvq, udp_sendq));

    info!("accepting {} simultaneous transfers", config.nb_transfers);
    for _ in 0..config.nb_transfers {
        let connect_recvq = connect_recvq.clone();
        let tcp_client_config = tcp_client_config.clone();
        let tcp_sendq = tcp_sendq.clone();
        thread::spawn(move || connect_loop(connect_recvq, tcp_client_config, tcp_sendq));
    }

    let tcp_listener = match TcpListener::bind(config.from_tcp) {
        Err(e) => {
            error!("failed to bind TCP {}: {}", config.from_tcp, e);
            return;
        }
        Ok(listener) => listener,
    };

    for client in tcp_listener.incoming() {
        match client {
            Err(e) => error!("failed to accept client: {e}"),
            Ok(client) => {
                if let Err(e) = connect_sendq.send(client) {
                    error!("failed to send client to connect queue: {e}");
                }
            }
        }
    }
}
