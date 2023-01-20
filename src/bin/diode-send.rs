use clap::{Arg, Command};
use crossbeam_channel::{bounded, unbounded, Receiver, RecvError, Sender};
use diode::{
    protocol, semaphore,
    send::{encoding, tcp_client, udp_send},
};
use log::{debug, error, info};
use std::{
    env, fmt,
    net::{SocketAddr, TcpListener, TcpStream},
    str::FromStr,
    thread,
    time::Duration,
};

struct Config {
    from_tcp: SocketAddr,
    from_tcp_buffer_size: usize,

    nb_clients: u16,
    nb_multiplex: u16,

    encoding_block_size: u64,
    repair_block_size: u32,
    flush_timeout: Duration,

    to_udp: SocketAddr,
    to_udp_mtu: u16,
}

fn command_args() -> Config {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_tcp")
                .long("from_tcp")
                .value_name("ip:port")
                .default_value("127.0.0.1:5000")
                .help("From where to read data"),
        )
        .arg(
            Arg::new("from_tcp_buffer_size")
                .long("from_tcp_buffer_size")
                .value_name("nb_bytes")
                .default_value("15000") // mtu * 10
                .value_parser(clap::value_parser!(usize))
                .help("Size of TCP read buffer"),
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
            Arg::new("nb_multiplex")
                .long("nb_multiplex")
                .value_name("nb")
                .default_value("2")
                .value_parser(clap::value_parser!(u16))
                .help("Number of multiplexed transfers"),
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
            Arg::new("flush_timeout")
                .long("flush_timeout")
                .value_name("nb_milliseconds")
                .default_value("100")
                .value_parser(clap::value_parser!(u64))
                .help("Duration in milliseconds after an incomplete RaptorQ block is flushed"),
        )
        .arg(
            Arg::new("to_udp")
                .long("to_udp")
                .value_name("ip:port")
                .default_value("127.0.0.1:6000")
                .help("Where to send data"),
        )
        .arg(
            Arg::new("to_udp_mtu")
                .long("to_udp_mtu")
                .value_name("nb_bytes")
                .default_value("1500") // mtu
                .value_parser(clap::value_parser!(u16))
                .help("MTU in bytes of output UDP link"),
        )
        .get_matches();

    let from_tcp = SocketAddr::from_str(args.get_one::<String>("from_tcp").expect("default"))
        .expect("invalid from_tcp parameter");
    let from_tcp_buffer_size = *args
        .get_one::<usize>("from_tcp_buffer_size")
        .expect("default");
    let nb_clients = *args.get_one::<u16>("nb_clients").expect("default");
    let nb_multiplex = *args.get_one::<u16>("nb_multiplex").expect("default");
    let encoding_block_size = *args.get_one::<u64>("encoding_block_size").expect("default");
    let repair_block_size = *args.get_one::<u32>("repair_block_size").expect("default");
    let flush_timeout =
        Duration::from_millis(*args.get_one::<u64>("flush_timeout").expect("default"));
    let to_udp = SocketAddr::from_str(args.get_one::<String>("to_udp").expect("default"))
        .expect("invalid to_udp parameter");
    let to_udp_mtu = *args.get_one::<u16>("to_udp_mtu").expect("default");

    Config {
        from_tcp,
        from_tcp_buffer_size,
        nb_clients,
        nb_multiplex,
        encoding_block_size,
        repair_block_size,
        flush_timeout,
        to_udp,
        to_udp_mtu,
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
    multiplex_control: semaphore::Semaphore,
    tcp_sendq: Sender<protocol::ClientMessage>,
) -> Result<(), Error> {
    loop {
        let client = connect_recvq.recv()?;
        tcp_client::new(&tcp_client_config, &multiplex_control, &tcp_sendq, client);
    }
}

fn connect_loop(
    connect_recvq: Receiver<TcpStream>,
    tcp_client_config: tcp_client::Config,
    multiplex_control: semaphore::Semaphore,
    tcp_senq: Sender<protocol::ClientMessage>,
) {
    if let Err(e) = connect_loop_aux(
        connect_recvq,
        tcp_client_config,
        multiplex_control,
        tcp_senq,
    ) {
        error!("failed to connect client: {e}");
    }
}

fn main() {
    let mut config = command_args();

    init_logger();

    config.encoding_block_size =
        protocol::adjust_encoding_block_size(config.to_udp_mtu, config.encoding_block_size);

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
        "encoding with block size of {} bytes and repair block size of {} bytes and a flush timeout of {} milliseconds",
        encoding_config.logical_block_size,
        encoding_config.repair_block_size,
        encoding_config.flush_timeout.as_millis(),
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
    let (tcp_sendq, tcp_recvq) = bounded::<protocol::ClientMessage>(config.nb_clients as usize);
    let (udp_sendq, udp_recvq) = unbounded::<udp_send::Message>();

    thread::spawn(move || udp_send::new(udp_send_config, udp_recvq));

    thread::spawn(move || encoding::new(encoding_config, tcp_recvq, udp_sendq));

    let multiplex_control = semaphore::Semaphore::new(config.nb_multiplex as usize);

    info!(
        "accepting {} simultaneous transfers with {} multiplexed transfers",
        config.nb_clients, config.nb_multiplex
    );

    for _ in 0..config.nb_clients {
        let connect_recvq = connect_recvq.clone();
        let tcp_client_config = tcp_client_config.clone();
        let multiplex_control = multiplex_control.clone();
        let tcp_sendq = tcp_sendq.clone();
        thread::spawn(move || {
            connect_loop(
                connect_recvq,
                tcp_client_config,
                multiplex_control,
                tcp_sendq,
            )
        });
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

fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env().unwrap();
    } else {
        simple_logger::init_with_level(log::Level::Info).unwrap();
    }
}
