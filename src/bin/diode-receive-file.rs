use clap::{Arg, Command};
use diode::file::{self, receive as frecv};
use log::{error, info};
use std::{
    env,
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    str::FromStr,
    thread,
};

fn command_args() -> frecv::Config {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_tcp")
                .long("from_tcp")
                .value_name("ip:port")
                .default_value("127.0.0.1:7000")
                .help("Address and port to listen for diode-receive"),
        )
        .arg(
            Arg::new("buffer_size")
                .long("buffer_size")
                .value_name("nb_bytes")
                .default_value("4194304") // 4096 * 1024
                .value_parser(clap::value_parser!(usize))
                .help("Size of TCP write buffer"),
        )
        .arg(
            Arg::new("output_directory")
                .value_name("dir")
                .default_value(".")
                .help("Output directory"),
        )
        .get_matches();

    let from_tcp = SocketAddr::from_str(args.get_one::<String>("from_tcp").expect("default"))
        .expect("invalid from_tcp parameter");
    let buffer_size = *args.get_one::<usize>("buffer_size").expect("default");
    let output_directory =
        PathBuf::from(args.get_one::<String>("output_directory").expect("default"));

    frecv::Config {
        from_tcp,
        buffer_size,
        output_directory,
    }
}

fn handle_client(config: &frecv::Config, client: TcpStream) {
    match frecv::receive_file(config, client) {
        Err(e) => error!("{e}"),
        Ok(total) => info!("file received, {total} bytes received"),
    }
}

fn main_loop(config: frecv::Config) -> Result<(), file::Error> {
    if !config.output_directory.is_dir() {
        return Err(file::Error::Other(
            "output_directory is not a directory".to_string(),
        ));
    }

    let server = TcpListener::bind(config.from_tcp)?;

    thread::scope(|scope| {
        for client in server.incoming() {
            match client {
                Err(e) => error!("failed to accept client: {e}"),
                Ok(client) => {
                    scope.spawn(|| handle_client(&config, client));
                }
            }
        }
    });

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
