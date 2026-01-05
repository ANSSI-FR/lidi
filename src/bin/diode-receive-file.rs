use clap::Parser;
use diode::aux::{self, file};
use std::{net, path};

#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct Listeners {
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port to accept TCP connections from diode-receive"
    )]
    from_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path of Unix socket to accept Unix connections from diode-receive"
    )]
    from_unix: Option<path::PathBuf>,
}

#[derive(Parser)]
#[clap(about = "Receive file(s) sent by diode-send-file through lidi.")]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Off|Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(flatten)]
    from: Listeners,
    #[clap(
        default_value = "4194304",
        value_name = "bytes",
        long,
        help = "Size of client write buffer"
    )]
    buffer_size: usize,
    #[clap(long, help = "Verify the hash of file content")]
    hash: bool,
    #[clap(default_value = ".", help = "Output directory")]
    output_directory: path::PathBuf,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = diode::init_logger(args.log_level, None, false) {
        eprintln!("failed to initialize logger: {e}");
        return;
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let diode = aux::DiodeReceive {
        from_tcp: args.from.from_tcp,
        from_unix: args.from.from_unix,
    };

    let config = file::Config {
        diode,
        buffer_size: args.buffer_size,
        hash: args.hash,
    };

    if let Err(e) = file::receive::receive_files(&config, &args.output_directory) {
        log::error!("{e}");
    }
}
