mod decoding;
mod deserialize;
mod tcp_serve;

use clap::{Arg, ArgAction, Command};
use crossbeam_channel::{unbounded, SendError};
use log::{debug, error, info};
use std::{
    fmt, io,
    net::{self, SocketAddr, UdpSocket},
    os::unix::net::UnixStream,
    str::FromStr,
    thread,
};

struct Config {
    from_udp: SocketAddr,
    from_udp_mtu: u16,

    encoding_block_size: u64,
    flush_timeout: u64,

    to_tcp: SocketAddr,
    to_tcp_buffer_size: usize,
    abort_timeout: u64,
}

impl Default for Config {
    fn default() -> Self {
        let mtu = 1500;
        Self {
            from_udp: SocketAddr::from_str("127.0.0.1:6000").unwrap(),
            from_udp_mtu: mtu as u16,

            encoding_block_size: (mtu * 40) as u64, //optimal parameter -- to align with other size !
            flush_timeout: 1,

            to_tcp: SocketAddr::from_str("127.0.0.1:7000").unwrap(),
            to_tcp_buffer_size: mtu * 10,
            abort_timeout: 60,
        }
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

fn command_args(config: &mut Config) {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_udp")
                .long("from_udp")
                .action(ArgAction::Set)
                .value_name("ip:port")
                .help("From where to read data"),
        )
        .arg(
            Arg::new("from_udp_mtu")
                .long("from_udp_mtu")
                .action(ArgAction::Set)
                .value_name("nb_bytes")
                .value_parser(clap::value_parser!(u16))
                .help("MTU of the incoming UDP link"),
        )
        .arg(
            Arg::new("encoding_block_size")
                .long("encoding_block_size")
                .action(ArgAction::Set)
                .value_name("nb_bytes")
                .value_parser(clap::value_parser!(u64))
                .help("Size of RaptorQ block"),
        )
        .arg(
            Arg::new("flush_timeout")
                .long("flush_timeout")
                .action(ArgAction::Set)
                .value_name("nb_seconds")
                .value_parser(clap::value_parser!(u64))
                .help(
                    "Duration in seconds after the last received complete RaptorQ block is flushed",
                ),
        )
        .arg(
            Arg::new("to_tcp")
                .long("to_tcp")
                .action(ArgAction::Set)
                .value_name("ip:port")
                .help("Where to send data"),
        )
        .arg(
            Arg::new("to_tcp_buffer_size")
                .long("to_tcp_buffer_size")
                .action(ArgAction::Set)
                .value_name("nb_bytes")
                .value_parser(clap::value_parser!(usize))
                .help("Size of TCP write buffer"),
        )
        .arg(
            Arg::new("abort_timeout")
                .long("abnrt_timeout")
                .action(ArgAction::Set)
                .value_name("nb_seconds")
                .value_parser(clap::value_parser!(u64))
                .help("Duration in seconds after a transfer without incoming data is aborted"),
        )
        .get_matches();

    if let Some(p) = args.get_one::<String>("from_udp") {
        let p = SocketAddr::from_str(p).expect("invalid from_udp parameter");
        config.from_udp = p;
    }

    if let Some(p) = args.get_one::<u16>("from_udp_mtu") {
        config.from_udp_mtu = *p;
    }

    if let Some(p) = args.get_one::<u64>("encoding_block_size") {
        config.encoding_block_size = *p;
    }

    if let Some(p) = args.get_one::<u64>("flush_timeout") {
        config.flush_timeout = *p;
    }

    if let Some(p) = args.get_one::<String>("to_tcp") {
        let p = SocketAddr::from_str(p).expect("invalid to_tcp parameter");
        config.to_tcp = p;
    }

    if let Some(p) = args.get_one::<usize>("to_tcp_buffer_size") {
        config.to_tcp_buffer_size = *p;
    }

    if let Some(p) = args.get_one::<u64>("abort_timeout") {
        config.abort_timeout = *p;
    }
}

fn main_loop(config: Config) -> Result<(), Error> {
    info!("listening for UDP packets at {}", config.from_udp);

    let socket = UdpSocket::bind(config.from_udp)?;

    let (decoding_sends, decoding_recvs) = UnixStream::pair()?;

    let (udp_sendq, udp_recvq) = unbounded::<decoding::Message>();

    let deserialize_config = deserialize::Config {
        logical_block_size: config.encoding_block_size,
        to_tcp: config.to_tcp,
        to_tcp_buffer_size: config.to_tcp_buffer_size,
        abort_timeout: config.abort_timeout,
    };

    thread::spawn(move || deserialize::new(deserialize_config, decoding_recvs));

    let decoding_config = decoding::Config {
        logical_block_size: config.encoding_block_size,
        input_mtu: config.from_udp_mtu,
        flush_timeout: config.flush_timeout,
    };

    info!(
        "decoding with block size of {} bytes and a flush timeout of {} second(s)",
        decoding_config.logical_block_size, decoding_config.flush_timeout,
    );

    thread::spawn(move || decoding::new(decoding_config, udp_recvq, decoding_sends));

    info!(
        "sending TCP traffic to {} with abort timeout of {} second(s)",
        config.to_tcp, config.abort_timeout,
    );

    let mut buffer = vec![0; config.from_udp_mtu as usize];

    loop {
        let (nread, _) = socket.recv_from(&mut buffer)?;
        let packet = decoding::Message::deserialize(&buffer[..nread]);
        udp_sendq.send(packet)?;
    }
}

fn main() {
    let mut config = Config::default();

    command_args(&mut config);

    protocol::init_logger();

    config.encoding_block_size =
        protocol::adjust_encoding_block_size(config.from_udp_mtu, config.encoding_block_size);

    debug!(
        "adjusting encoding_block_size to {} bytes",
        config.encoding_block_size
    );

    if let Err(e) = main_loop(config) {
        error!("failed to launch main_loop: {e}");
    }
}
