use diode::{
    init_logger, init_metrics,
    protocol::{self, Header},
    send::{self, SenderConfig},
};
use std::str::FromStr;
use std::{net::SocketAddr, thread, time::Duration};

use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct DiodeConfig {
    /// TCP server socket to create to accept data
    #[arg(short, long, default_value_t = String::from("127.0.0.1:5001"))]
    bind_tcp: String,
    /// Size of RaptorQ block, in bytes
    #[arg(short, long, default_value_t = 60000)]
    encoding_block_size: u64,
    /// Size of repair data, in bytes
    #[arg(short, long, default_value_t = 6000)]
    repair_block_size: u32,
    /// Number of parallel RaptorQ encoding threads. One thread encodes around 3 Gb/s.
    #[arg(short, long, default_value_t = 4)]
    nb_threads: u8,
    /// IP address and port where to send UDP packets to diode-receive
    #[arg(short, long)]
    to_udp: String,
    /// Binding IP for UDP traffic
    #[arg(short, long, default_value_t = String::from("0.0.0.0:0"))]
    bind_udp: String,
    /// MTU of the output UDP link
    #[arg(short, long, default_value_t = 1500)]
    udp_mtu: u16,
    /// heartbeat period in ms
    #[arg(short, long, default_value_t = 1000)]
    heartbeat: u16,
    /// ratelimit TCP session speed (in bit/s)
    #[arg(short, long)]
    max_bandwidth: Option<f64>,
    /// prometheus port
    #[arg(short, long)]
    metrics: Option<String>,
    /// Path to log configuration file
    #[arg(short, long)]
    log_config: Option<String>,
    /// Verbosity level. Using it multiple times adds more logs.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,
}

impl From<DiodeConfig> for SenderConfig {
    fn from(config: DiodeConfig) -> Self {
        let object_transmission_info =
            protocol::object_transmission_information(config.udp_mtu, config.encoding_block_size);

        let from_buffer_size = object_transmission_info.transfer_length() as u32;
        let to_max_messages = protocol::nb_encoding_packets(&object_transmission_info) as u16
            + protocol::nb_repair_packets(&object_transmission_info, config.repair_block_size)
                as u16;

        let mut to_encoding = vec![];
        let mut for_encoding = vec![];

        (0..config.nb_threads).for_each(|_| {
            let (tx, rx) = crossbeam_channel::unbounded::<(Header, Vec<u8>)>();
            to_encoding.push(tx);
            for_encoding.push(rx);
        });

        Self {
            // from command line
            encoding_block_size: config.encoding_block_size,
            repair_block_size: config.repair_block_size,
            nb_threads: config.nb_threads,
            hearbeat_interval: Duration::from_millis(config.heartbeat as _),
            bind_udp: SocketAddr::from_str(&config.bind_udp)
                .expect("cannot parse to_udp_bind address"),
            to_udp: SocketAddr::from_str(&config.to_udp).expect("cannot parse to_udp address"),
            to_udp_mtu: config.udp_mtu,
            from_tcp: SocketAddr::from_str(&config.bind_tcp)
                .expect("cannot parse from_tcp address"),
            // computed
            object_transmission_info,
            from_buffer_size,
            to_max_messages,
            to_encoding,
            for_encoding,
            max_bandwidth: config.max_bandwidth,
        }
    }
}

fn main() {
    let config = DiodeConfig::parse();

    init_logger(config.log_config.as_ref(), config.debug);
    init_metrics(config.metrics.as_ref());

    let sender = send::SenderConfig::from(config);

    thread::scope(|scope| {
        if let Err(e) = sender.start(scope) {
            log::error!("failed to start diode sender: {e}");
        }
    });
}
