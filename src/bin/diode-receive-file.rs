use clap::{Arg, ArgAction, Command};
use diode::aux::{self, file};
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
            Arg::new("buffer_size")
                .long("buffer_size")
                .value_name("nb_bytes")
                .default_value("4194304") // 4096 * 1024
                .value_parser(clap::value_parser!(usize))
                .help("Size of client write buffer"),
        )
        .arg(
            Arg::new("hash")
                .long("hash")
                .action(ArgAction::SetTrue)
                .default_value("false")
                .value_parser(clap::value_parser!(bool))
                .help("Verify the hash of file content (default is false)"),
        )
        .arg(
            Arg::new("output_directory")
                .value_name("dir")
                .default_value(".")
                .help("Output directory"),
        )
        .get_matches();

    let from_tcp = args
        .get_one::<String>("from_tcp")
        .map(|s| net::SocketAddr::from_str(s).expect("invalid from_tcp parameter"));
    let from_unix = args
        .get_one::<String>("from_unix")
        .map(|s| path::PathBuf::from_str(s).expect("invalid from_unix parameter"));
    let buffer_size = *args.get_one::<usize>("buffer_size").expect("default");
    let hash = args.get_one::<bool>("hash").copied().expect("default");
    let output_directory =
        path::PathBuf::from(args.get_one::<String>("output_directory").expect("default"));

    let diode = aux::DiodeReceive {
        from_tcp,
        from_unix,
    };

    let config = file::Config {
        diode,
        buffer_size,
        hash,
    };

    init_logger();

    if let Err(e) = file::receive::receive_files(&config, &output_directory) {
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
