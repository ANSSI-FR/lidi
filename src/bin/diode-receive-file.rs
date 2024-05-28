use diode::{file, init_logger};
use std::{net, path, str::FromStr};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct SendFileConfig {
    /// IP address and port to accept TCP connections from diode-receive (default 127.0.0.1:5002)
    #[arg(long, default_value_t = String::from("127.0.0.1:5002"))]
    bind_tcp: String,
    /// Size of file buffer
    #[arg(long, default_value_t = 8196)]
    buffer_size: usize,
    /// Verify the hash of file content (default is false)
    #[arg(long, default_value_t = false)]
    hash: bool,
    /// Path to log configuration file
    #[arg(short, long)]
    log_config: Option<String>,
    /// Verbosity level. Using it multiple times adds more logs.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,
    /// Output directory
    #[arg()]
    dir: String,
}

fn main() {
    let args = SendFileConfig::parse();

    let from_tcp = net::SocketAddr::from_str(&args.bind_tcp).expect("invalid from_tcp parameter");
    let buffer_size = args.buffer_size;
    let hash = args.hash;
    let output_directory = path::PathBuf::from(args.dir);

    let config = file::Config {
        diode: from_tcp,
        buffer_size,
        hash,
    };

    init_logger(args.log_config.as_ref(), args.debug);

    loop {
        if let Err(e) = file::receive::receive_files(&config, &output_directory) {
            log::error!("{e}");
        }
    }
}
