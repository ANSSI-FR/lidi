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
        .arg(
            Arg::new("log_file_path")
                .long("log_file_path")
                .value_name("path")
                .default_value(None)
                .help("Path to the log file"),
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
    let log_file_path = args
        .get_one::<String>("log_file_path")
        .map(|s| path::PathBuf::from_str(s).expect("log_file_path must point to a valid path"));
    let diode = if let Some(to_tcp) = to_tcp {
        aux::DiodeSend::Tcp(to_tcp)
    } else {
        aux::DiodeSend::Unix(to_unix.expect("to_tcp and to_unix are mutually exclusive"))
    };

    let config = udp::Config {
        diode,
        buffer_size: u16::MAX as usize,
    };

    let _ = diode::init_logger(log_file_path);

    if let Err(e) = udp::send::send(&config, from_udp) {
        log::error!("{e}");
    }
}
