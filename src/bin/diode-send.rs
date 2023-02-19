use clap::{Arg, ArgAction, Command};
use crossbeam_channel::{bounded, unbounded, Receiver, RecvError, Sender};
use crossbeam_utils::atomic::AtomicCell;
use diode::{
    protocol, semaphore,
    send::{encoding, heartbeat, tcp_client, udp_send},
};
use log::{error, info};
use raptorq::EncodingPacket;
use std::{
    env, fmt,
    net::{SocketAddr, TcpListener, TcpStream},
    str::FromStr,
    sync::Mutex,
    thread,
    time::Duration,
};

struct Config {
    from_tcp: SocketAddr,

    nb_clients: u16,
    nb_multiplex: u16,

    nb_encoding_threads: u8,

    encoding_block_size: u64,
    repair_block_size: u32,

    to_bind: Vec<SocketAddr>,
    to_udp: SocketAddr,
    to_udp_mtu: u16,

    heartbeat: Duration,
}

impl Config {
    fn adjust(&mut self) {
        let oti =
            protocol::object_transmission_information(self.to_udp_mtu, self.encoding_block_size);

        let packet_size = protocol::packet_size(&oti);
        let nb_encoding_packets = protocol::nb_encoding_packets(&oti) * self.to_bind.len() as u64;
        let nb_repair_packets =
            protocol::nb_repair_packets(&oti, self.repair_block_size) * self.to_bind.len() as u32;

        self.encoding_block_size = nb_encoding_packets * packet_size as u64;
        self.repair_block_size = nb_repair_packets * packet_size as u32;
    }
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
                .action(ArgAction::Append)
                .default_values(vec!["0.0.0.0:0"])
                .help("Binding IP; multiple values accepted"),
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
        .arg(
            Arg::new("heartbeat")
                .long("heartbeat")
                .value_name("nb_secq")
                .default_value("5")
                .value_parser(clap::value_parser!(u16))
                .help("Duration in seconds between heartbeat messages"),
        )
        .get_matches();

    let from_tcp = SocketAddr::from_str(args.get_one::<String>("from_tcp").expect("default"))
        .expect("invalid from_tcp parameter");
    let nb_clients = *args.get_one::<u16>("nb_clients").expect("default");
    let nb_multiplex = *args.get_one::<u16>("nb_multiplex").expect("default");
    let nb_encoding_threads = *args.get_one::<u8>("nb_encoding_threads").expect("default");
    let encoding_block_size = *args.get_one::<u64>("encoding_block_size").expect("default");
    let repair_block_size = *args.get_one::<u32>("repair_block_size").expect("default");
    let to_bind: Vec<SocketAddr> = args
        .get_many::<String>("to_bind")
        .expect("default")
        .map(|addr| SocketAddr::from_str(addr).expect("invalid to_bind address"))
        .collect();
    let to_udp = SocketAddr::from_str(args.get_one::<String>("to_udp").expect("default"))
        .expect("invalid to_udp parameter");
    let to_udp_mtu = *args.get_one::<u16>("to_udp_mtu").expect("default");
    let heartbeat = *args.get_one::<u16>("heartbeat").expect("default");

    Config {
        from_tcp,
        nb_clients,
        nb_multiplex,
        nb_encoding_threads,
        encoding_block_size,
        repair_block_size,
        to_bind,
        to_udp,
        to_udp_mtu,
        heartbeat: Duration::from_secs(heartbeat as u64),
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
    connect_recvq: &Receiver<TcpStream>,
    tcp_client_config: &tcp_client::Config,
    multiplex_control: semaphore::Semaphore,
    tcp_sendq: &Sender<protocol::Message>,
) -> Result<(), Error> {
    loop {
        let client = connect_recvq.recv()?;
        tcp_client::new(
            tcp_client_config,
            &multiplex_control,
            tcp_sendq.clone(),
            client,
        );
    }
}

fn connect_loop(
    connect_recvq: &Receiver<TcpStream>,
    tcp_client_config: &tcp_client::Config,
    multiplex_control: semaphore::Semaphore,
    tcp_senq: &Sender<protocol::Message>,
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

    config.adjust();

    let object_transmission_info =
        protocol::object_transmission_information(config.to_udp_mtu, config.encoding_block_size);

    let tcp_client_config = tcp_client::Config {
        buffer_size: (object_transmission_info.transfer_length()
            - protocol::Message::serialize_overhead() as u64) as u32,
    };

    let encoding_config = encoding::Config {
        object_transmission_info,
        repair_block_size: config.repair_block_size,
    };

    let (connect_sendq, connect_recvq) = bounded::<TcpStream>(1);
    let (tcp_sendq, tcp_recvq) = bounded::<protocol::Message>(config.nb_clients as usize);
    let (udp_sendq, udp_recvq) = unbounded::<Vec<EncodingPacket>>();

    let max_messages = (protocol::nb_encoding_packets(&object_transmission_info) as u16
        + protocol::nb_repair_packets(&object_transmission_info, config.repair_block_size) as u16)
        / config.to_bind.len() as u16;

    let multiplex_control = semaphore::Semaphore::new(config.nb_multiplex as usize);

    let block_to_encode = AtomicCell::new(0);
    let block_to_send = Mutex::new(0);

    let heartbeat_config = heartbeat::Config {
        buffer_size: tcp_client_config.buffer_size,
        duration: config.heartbeat,
    };

    let udp_send_config = udp_send::Config {
        to_bind: config.to_bind,
        to_udp: config.to_udp,
        mtu: config.to_udp_mtu,
        max_messages,
        encoding_block_size: config.encoding_block_size,
        repair_block_size: config.repair_block_size,
    };

    thread::scope(|scope| {
        thread::Builder::new()
            .name("udp-send".into())
            .spawn_scoped(scope, || udp_send::new(udp_send_config, &udp_recvq))
            .unwrap();

        info!("TCP buffer size is {} bytes", tcp_client_config.buffer_size);

        info!(
            "accepting {} simultaneous transfers with {} multiplexed transfers",
            config.nb_clients, config.nb_multiplex
        );

        for _ in 0..config.nb_clients {
            thread::Builder::new()
                .name("tcp-client".to_string())
                .spawn_scoped(scope, || {
                    connect_loop(
                        &connect_recvq,
                        &tcp_client_config,
                        multiplex_control.clone(),
                        &tcp_sendq,
                    )
                })
                .unwrap();
        }

        info!(
            "encoding will produce {} packets ({} bytes per block) + {} repair packets",
            protocol::nb_encoding_packets(&object_transmission_info),
            config.encoding_block_size,
            protocol::nb_repair_packets(&object_transmission_info, config.repair_block_size),
        );

        for i in 0..config.nb_encoding_threads {
            thread::Builder::new()
                .name(format!("encoding_{i}"))
                .spawn_scoped(scope, || {
                    encoding::new(
                        &encoding_config,
                        &block_to_encode,
                        &block_to_send,
                        &tcp_recvq,
                        &udp_sendq,
                    )
                })
                .unwrap();
        }

        info!("accepting TCP clients at {}", config.from_tcp);

        let tcp_listener = match TcpListener::bind(config.from_tcp) {
            Err(e) => {
                error!("failed to bind TCP {}: {}", config.from_tcp, e);
                return;
            }
            Ok(listener) => listener,
        };

        thread::Builder::new()
            .name("heartbeat".into())
            .spawn_scoped(scope, || heartbeat::new(&heartbeat_config, &tcp_sendq))
            .unwrap();

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
    });
}

fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env().unwrap();
    } else {
        simple_logger::init_with_level(log::Level::Info).unwrap();
    }
}
