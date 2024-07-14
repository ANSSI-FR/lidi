use clap::{Arg, ArgGroup, Command};
use diode::aux::{self, udp};
use std::{env, net, path, str::FromStr};

fn main() {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("to_tcp")
                .long("to_tcp")
                .value_name("ip:port")
                .help("IP address and port to connect in TCP to diode-send"),
        )
        .arg(
            Arg::new("to_unix")
                .long("to_unix")
                .value_name("path")
                .help("Path of Unix socket to connect to diode-send"),
        )
        .group(
            ArgGroup::new("to")
                .required(true)
                .args(["to_tcp", "to_unix"]),
        )
        .arg(
            Arg::new("from_udp")
                .long("from_udp")
                .value_name("ip:port")
                .required(true)
                .help("IP address and port to receive UDP packets"),
        )
        .get_matches();

    let to_tcp = args
        .get_one::<String>("to_tcp")
        .map(|s| net::SocketAddr::from_str(s).expect("to_tcp must be of the form ip:port"));
    let to_unix = args
        .get_one::<String>("to_unix")
        .map(|s| path::PathBuf::from_str(s).expect("to_unix must point to a valid path"));
    let from_udp = args
        .get_one::<String>("from_udp")
        .map(|s| net::SocketAddr::from_str(s).expect("from_udp must be of the form ip:port"))
        .expect("from_udp parameter is required");

    let diode = if let Some(to_tcp) = to_tcp {
        aux::DiodeSend::Tcp(to_tcp)
    } else {
        aux::DiodeSend::Unix(to_unix.expect("to_tcp and to_unix are mutually exclusive"))
    };

    let config = udp::Config {
        diode,
        buffer_size: u16::MAX as usize,
    };

    init_logger();

    if let Err(e) = udp::send::send(&config, from_udp) {
        log::error!("{e}");
    }
}

fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env()
    } else {
        simple_logger::init_with_level(log::Level::Info)
    }
    .expect("logger initialization")
}
