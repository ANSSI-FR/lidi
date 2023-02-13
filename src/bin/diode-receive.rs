use clap::{Arg, Command};
use crossbeam_channel::{unbounded, SendError};
use diode::protocol;
use diode::receive::decoding;
use diode::receive::deserialize;
use log::{error, info};
use std::{
    env, fmt, io,
    net::{self, SocketAddr, UdpSocket},
    os::unix::net::UnixStream,
    str::FromStr,
    thread,
    time::Duration,
};

struct Config {
    from_udp: SocketAddr,
    from_udp_mtu: u16,

    nb_multiplex: u16,

    encoding_block_size: u64,
    flush_timeout: Duration,

    to_tcp: SocketAddr,
    to_tcp_buffer_size: usize,
    abort_timeout: Duration,
}

fn command_args() -> Config {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_udp")
                .long("from_udp")
                .value_name("ip:port")
                .default_value("127.0.0.1:6000")
                .help("From where to read data"),
        )
        .arg(
            Arg::new("from_udp_mtu")
                .long("from_udp_mtu")
                .value_name("nb_bytes")
                .default_value("1500") // mtu
                .value_parser(clap::value_parser!(u16))
                .help("MTU of the incoming UDP link"),
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
                .help("Size of RaptorQ block"),
        )
        .arg(
            Arg::new("flush_timeout")
                .long("flush_timeout")
                .value_name("nb_milliseconds")
                .default_value("500")
                .value_parser(clap::value_parser!(u64))
                .help("Duration in milliseconds after resetting RaptorQ status"),
        )
        .arg(
            Arg::new("to_tcp")
                .long("to_tcp")
                .value_name("ip:port")
                .default_value("127.0.0.1:7000")
                .help("Where to send data"),
        )
        .arg(
            Arg::new("to_tcp_buffer_size")
                .long("to_tcp_buffer_size")
                .value_name("nb_bytes")
                .default_value("15000") // mtu * 10,
                .value_parser(clap::value_parser!(usize))
                .help("Size of TCP write buffer"),
        )
        .arg(
            Arg::new("abort_timeout")
                .long("abort_timeout")
                .value_name("nb_seconds")
                .default_value("10")
                .value_parser(clap::value_parser!(u64))
                .help("Duration in seconds after a transfer without incoming data is aborted"),
        )
        .get_matches();

    let from_udp = SocketAddr::from_str(args.get_one::<String>("from_udp").expect("default"))
        .expect("invalid from_udp_parameter");
    let from_udp_mtu = *args.get_one::<u16>("from_udp_mtu").expect("default");
    let nb_multiplex = *args.get_one::<u16>("nb_multiplex").expect("default");
    let encoding_block_size = *args.get_one::<u64>("encoding_block_size").expect("default");
    let flush_timeout =
        Duration::from_millis(*args.get_one::<u64>("flush_timeout").expect("default"));
    let to_tcp = SocketAddr::from_str(args.get_one::<String>("to_tcp").expect("default"))
        .expect("invalid to_tcp parameter");
    let to_tcp_buffer_size = *args
        .get_one::<usize>("to_tcp_buffer_size")
        .expect("default");
    let abort_timeout =
        Duration::from_secs(*args.get_one::<u64>("abort_timeout").expect("default"));

    Config {
        from_udp,
        from_udp_mtu,
        nb_multiplex,
        encoding_block_size,
        flush_timeout,
        to_tcp,
        to_tcp_buffer_size,
        abort_timeout,
    }
}

enum Error {
    Io(io::Error),
    AddrParseError(net::AddrParseError),
    Crossbeam(SendError<decoding::Message>),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::AddrParseError(e) => write!(fmt, "address parse error: {e}"),
            Self::Crossbeam(e) => write!(fmt, "crossbeam send error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<net::AddrParseError> for Error {
    fn from(e: net::AddrParseError) -> Self {
        Self::AddrParseError(e)
    }
}

impl From<SendError<decoding::Message>> for Error {
    fn from(e: SendError<decoding::Message>) -> Self {
        Self::Crossbeam(e)
    }
}

fn main_loop(config: Config) -> Result<(), Error> {
    info!("listening for UDP packets at {}", config.from_udp);

    let socket = UdpSocket::bind(config.from_udp)?;

    let (decoding_sends, decoding_recvs) = UnixStream::pair()?;

    let (udp_sendq, udp_recvq) = unbounded::<decoding::Message>();

    let deserialize_config = deserialize::Config {
        nb_multiplex: config.nb_multiplex,
        logical_block_size: config.encoding_block_size,
        to_tcp: config.to_tcp,
        to_tcp_buffer_size: config.to_tcp_buffer_size,
        abort_timeout: config.abort_timeout,
    };

    thread::Builder::new()
        .name("deserialize".to_string())
        .spawn(move || deserialize::new(deserialize_config, decoding_recvs))
        .unwrap();

    let object_transmission_info =
        protocol::object_transmission_information(config.from_udp_mtu, config.encoding_block_size);

    let decoding_config = decoding::Config {
        object_transmission_info,
        flush_timeout: config.flush_timeout,
    };

    thread::Builder::new()
        .name("decoding".to_string())
        .spawn(move || decoding::new(decoding_config, udp_recvq, decoding_sends))
        .unwrap();

    info!(
        "sending TCP traffic to {} with abort timeout of {} second(s) an {} multiplexed transfers",
        config.to_tcp,
        config.abort_timeout.as_secs(),
        config.nb_multiplex,
    );

    let mut buffer = vec![0; config.from_udp_mtu as usize];

    loop {
        let nread = socket.recv(&mut buffer)?;
        let packet = decoding::Message::deserialize(&buffer[..nread]);
        udp_sendq.send(packet)?;
    }
}

fn main() {
    let config = command_args();

    init_logger();

    if let Err(e) = main_loop(config) {
        error!("failed to launch main_loop: {e}");
    }
}

fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env().unwrap();
    } else {
        simple_logger::init_with_level(log::Level::Info).unwrap();
    }
}
