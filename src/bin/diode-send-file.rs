use clap::{Arg, ArgAction, ArgGroup, Command};
use diode::aux::{self, file};
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
            Arg::new("buffer_size")
                .long("buffer_size")
                .value_name("nb_bytes")
                .default_value("4194304") // 4096 * 1024
                .value_parser(clap::value_parser!(usize))
                .help("Size of file read/client write buffer"),
        )
        .arg(
            Arg::new("hash")
                .long("hash")
                .action(ArgAction::SetTrue)
                .default_value("false")
                .value_parser(clap::value_parser!(bool))
                .help("Compute a hash of file content (default is false)"),
        )
        .arg(
            Arg::new("file")
                .action(ArgAction::Append)
                .allow_hyphen_values(true)
                .required(true),
        )
        .get_matches();

    let to_tcp = args
        .get_one::<String>("to_tcp")
        .map(|s| net::SocketAddr::from_str(s).expect("to_tcp must be of the form ip:port"));
    let to_unix = args
        .get_one::<String>("to_unix")
        .map(|s| path::PathBuf::from_str(s).expect("to_unix must point to a valid path"));
    let buffer_size = *args.get_one::<usize>("buffer_size").expect("default");
    let hash = args.get_one::<bool>("hash").copied().expect("default");
    let files = args
        .get_many("file")
        .expect("required")
        .cloned()
        .collect::<Vec<_>>();

    let diode = if let Some(to_tcp) = to_tcp {
        aux::DiodeSend::Tcp(to_tcp)
    } else {
        aux::DiodeSend::Unix(to_unix.expect("to_tcp and to_unix are mutually exclusive"))
    };

    let config = file::Config {
        diode,
        buffer_size,
        hash,
    };

    diode::init_logger();

    if let Err(e) = file::send::send_files(&config, &files) {
        log::error!("{e}");
    }
}
