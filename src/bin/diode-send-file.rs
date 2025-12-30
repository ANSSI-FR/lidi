use clap::Parser;
use diode::aux::{self, file};
use std::{net, path};

#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct Clients {
    #[clap(
        value_name = "ip:port",
        long,
        help = "TCP address and port to connect to diode-send"
    )]
    to_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path to Unix socket to connect to diode-send"
    )]
    to_unix: Option<path::PathBuf>,
}

#[derive(Parser)]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(flatten)]
    to: Clients,
    #[clap(
        default_value = "4194304",
        value_name = "bytes",
        long,
        help = "Size of client internal read/write buffer"
    )]
    buffer_size: usize,
    #[clap(long, help = "Compute and send the hash of file content")]
    hash: bool,
    #[clap(help = "Files to send")]
    files: Vec<String>,
}

fn main() {
    let args = Args::parse();

    diode::init_logger(args.log_level);

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let diode = if let Some(to_tcp) = args.to.to_tcp {
        aux::DiodeSend::Tcp(to_tcp)
    } else if let Some(to_unix) = args.to.to_unix {
        aux::DiodeSend::Unix(to_unix)
    } else {
        unreachable!()
    };

    let config = file::Config {
        diode,
        buffer_size: args.buffer_size,
        hash: args.hash,
    };

    if let Err(e) = file::send::send_files(&config, &args.files) {
        log::error!("{e}");
    }
}
