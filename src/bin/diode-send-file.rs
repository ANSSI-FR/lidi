use diode::{file, init_logger};
use std::{net, str::FromStr};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct SendFileConfig {
    /// IP address and port to connect in TCP to diode-send (ex "127.0.0.1:5001")
    #[arg(short, long, default_value_t = String::from("127.0.0.1:5001"))]
    to_tcp: String,
    /// Size of file buffer
    #[arg(short, long, default_value_t = 8196)]
    buffer_size: usize,
    /// Compute a hash of file content (default is false)
    #[arg(short, long, default_value_t = false)]
    hash: bool,
    /// List of files to send
    #[arg()]
    file: Vec<String>,
    /// Path to log configuration file
    #[arg(short, long)]
    log_config: Option<String>,
    /// Verbosity level. Using it multiple times adds more logs.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,
}

fn main() {
    let args = SendFileConfig::parse();

    init_logger(args.log_config.as_ref(), args.debug);

    let to_tcp =
        net::SocketAddr::from_str(&args.to_tcp).expect("to_tcp must be of the form ip:port");
    let buffer_size = args.buffer_size;
    let hash = args.hash;
    let files = args.file;

    let config = file::Config {
        diode: to_tcp,
        buffer_size,
        hash,
    };

    if let Err(e) = file::send::send_files(&config, &files) {
        log::error!("{e}");
        std::process::exit(1);
    }
}
