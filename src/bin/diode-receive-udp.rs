use clap::{Arg, Command};
use diode::aux::{self, udp};
use std::{env, net, path, str::FromStr};

fn main() {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_tcp")
                .long("from_tcp")
                .value_name("ip:port")
                .default_value("127.0.0.1:7000")
                .help("IP address and port to accept TCP connections from diode-receive"),
        )
        .arg(
            Arg::new("from_unix")
                .long("from_unix")
                .value_name("path")
                .help("Path of Unix socket to accept Unix connections from diode-receive"),
        )
        .arg(
            Arg::new("to_udp_bind")
                .long("to_udp_bind")
                .value_name("ip:port")
                .required(true)
                .help("IP address and port to send UDP packets from"),
        )
        .arg(
            Arg::new("to_udp")
                .long("to_udp")
                .value_name("ip:port")
                .required(true)
                .help("IP address and port to send UDP packets to"),
        )
        .get_matches();

    let from_tcp = args
        .get_one::<String>("from_tcp")
        .map(|s| net::SocketAddr::from_str(s).expect("invalid from_tcp parameter"));
    let from_unix = args
        .get_one::<String>("from_unix")
        .map(|s| path::PathBuf::from_str(s).expect("invalid from_unix parameter"));
    let to_udp_bind = args
        .get_one::<String>("to_udp_bind")
        .map(|s| net::SocketAddr::from_str(s).expect("to_udp_bind must be of the form ip:port"))
        .expect("to_udp_bind parameter is required");
    let to_udp = args
        .get_one::<String>("to_udp")
        .map(|s| net::SocketAddr::from_str(s).expect("to_udp must be of the form ip:port"))
        .expect("to_udp parameter is required");

    let diode = aux::DiodeReceive {
        from_tcp,
        from_unix,
    };

    let config = udp::Config {
        diode,
        buffer_size: u16::MAX as usize,
    };

    diode::init_logger();

    if let Err(e) = udp::receive::receive(&config, to_udp_bind, to_udp) {
        log::error!("{e}");
    }
}
