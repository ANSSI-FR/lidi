use diode::{
    init_logger, init_metrics,
    protocol::{self, Header},
    receive::ReceiverConfig,
};
use std::{net::SocketAddr, str::FromStr, time::Duration};

use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct DiodeConfig {
    /// IP address and port to connect to TCP server
    #[arg(short, long, default_value_t = String::from("127.0.0.1:5002"))]
    to_tcp: String,
    /// Timeout before force incomplete block recovery (in ms)
    #[arg(short, long, default_value_t = 500)]
    flush_timeout: u16,
    /// Size of RaptorQ block, in bytes
    #[arg(short, long, default_value_t = 60000)]
    encoding_block_size: u64,
    /// Size of repair data, in bytes
    #[arg(short, long, default_value_t = 6000)]
    repair_block_size: u32,
    /// Number of parallel RX threads
    #[arg(short, long, default_value_t = 1)]
    nb_threads: u8,
    /// IP address and port where to send UDP packets to diode-receive
    #[arg(short, long)]
    bind_udp: String,
    /// MTU of the output UDP link
    #[arg(short, long, default_value_t = 1500)]
    udp_mtu: u16,
    /// heartbeat period in ms
    #[arg(short, long, default_value_t = 2000)]
    heartbeat_interval: u16,
    /// prometheus port
    #[arg(short, long)]
    metrics: Option<String>,
    /// Path to log configuration file
    #[arg(short, long)]
    log_config: Option<String>,
    /// Verbosity level. Using it multiple times adds more logs.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,
    /// Session expiration delay. Time to wait before changing session (in s). Default is 5.
    #[arg(short, long, default_value_t = 5)]
    pub session_expiration_delay: usize,
}

impl From<DiodeConfig> for ReceiverConfig {
    fn from(config: DiodeConfig) -> Self {
        let object_transmission_info =
            protocol::object_transmission_information(config.udp_mtu, config.encoding_block_size);

        let to_buffer_size = object_transmission_info.transfer_length() as _;

        let from_max_messages = protocol::nb_encoding_packets(&object_transmission_info) as u16
            + protocol::nb_repair_packets(&object_transmission_info, config.repair_block_size)
                as u16;

        let (to_reorder, for_reorder) = crossbeam_channel::unbounded::<(Header, Vec<u8>)>();

        Self {
            // from command line
            encoding_block_size: config.encoding_block_size,
            repair_block_size: config.repair_block_size,
            nb_threads: config.nb_threads,
            heartbeat_interval: Duration::from_millis(config.heartbeat_interval as _),
            from_udp: SocketAddr::from_str(&config.bind_udp)
                .expect("cannot parse from-udp address"),
            from_udp_mtu: config.udp_mtu,
            to_tcp: SocketAddr::from_str(&config.to_tcp).expect("cannot parse to-tcp address"),
            flush_timeout: Duration::from_millis(config.flush_timeout as _),
            // computed
            object_transmission_info,
            to_buffer_size,
            from_max_messages,
            to_reorder,
            for_reorder,
            session_expiration_delay: config.session_expiration_delay,
        }
    }
}

fn main() {
    let config = DiodeConfig::parse();

    init_logger(config.log_config.as_ref(), config.debug);
    init_metrics(config.metrics.as_ref());

    log::info!("sending traffic to {}", config.to_tcp);

    let receiver = ReceiverConfig::from(config);

    if let Err(e) = receiver.start() {
        log::error!("failed to start diode receiver: {e}");
    }
}
