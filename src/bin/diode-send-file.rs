use diode::file::{self, send as fsend};

use clap::{Arg, ArgAction, Command};
use log::{error, info};
use std::{env, net::SocketAddr, str::FromStr};

fn command_args() -> fsend::Config {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("to_tcp")
                .long("to_tcp")
                .value_name("ip:port")
                .default_value("127.0.0.1:5000")
                .help("Address and port to connect to diode-send"),
        )
        .arg(
            Arg::new("buffer_size")
                .long("buffer_size")
                .value_name("nb_bytes")
                .default_value("4194304") // 4096 * 1024
                .value_parser(clap::value_parser!(usize))
                .help("Size of file read/TCP write buffer"),
        )
        .arg(
            Arg::new("file")
                .action(ArgAction::Append)
                .allow_hyphen_values(true)
                .required(true),
        )
        .get_matches();

    let to_tcp = SocketAddr::from_str(args.get_one::<String>("to_tcp").expect("default"))
        .expect("invalid to_tcp parameter");
    let buffer_size = *args.get_one::<usize>("buffer_size").expect("default");
    let files = args.get_many("file").expect("required").cloned().collect();

    fsend::Config {
        to_tcp,
        buffer_size,
        files,
    }
}

fn main_loop(config: fsend::Config) -> Result<(), file::Error> {
    for file in &config.files {
        let total = fsend::send_file(&config, file)?;
        info!("file send, {total} bytes sent");
    }
    Ok(())
}

fn main() {
    let config = command_args();

    init_logger();

    if let Err(e) = main_loop(config) {
        error!("{e}");
    }
}

fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env().unwrap();
    } else {
        simple_logger::init_with_level(log::Level::Info).unwrap();
    }
}
