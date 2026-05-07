use clap::Parser;
use std::{net, path};

#[derive(clap::Args)]
#[group(required = true, multiple = false)]
#[allow(clippy::struct_field_names)]
struct Clients {
    #[clap(
        value_name = "ip:port",
        long,
        help = "TCP address and port to connect to lidi-send"
    )]
    to_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "ip:port",
        long,
        help = "TLS address and port to connect to lidi-send"
    )]
    to_tls: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path to Unix socket to connect to lidi-send"
    )]
    to_unix: Option<path::PathBuf>,
}

#[derive(Parser)]
#[clap(about = "Send a file to lidi-file-receive through lidi.")]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Off|Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(long, help = "Path to log4rs configuration file")]
    log_config: Option<path::PathBuf>,
    #[clap(flatten)]
    to: Clients,
    #[clap(
        default_value = "4194304",
        value_name = "bytes",
        long,
        help = "Size of client internal read/write buffer"
    )]
    buffer_size: usize,
    #[cfg(feature = "hash")]
    #[clap(long, help = "Compute and send the hash of file content")]
    hash: bool,
    #[clap(flatten)]
    tls: lidi_clients::Tls,
    #[clap(help = "Files to send")]
    files: Vec<path::PathBuf>,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = lidi_clients::init_logger(args.log_level, args.log_config.as_ref()) {
        eprintln!("failed to initialize logger: {e}");
        return;
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let diode = if let Some(to_tcp) = args.to.to_tcp {
        lidi_clients::DiodeSend::Tcp(to_tcp)
    } else if let Some(to_tls) = args.to.to_tls {
        lidi_clients::DiodeSend::Tls(to_tls)
    } else if let Some(to_unix) = args.to.to_unix {
        lidi_clients::DiodeSend::Unix(to_unix)
    } else {
        unreachable!()
    };

    let config = lidi_clients::file::Config {
        diode,
        buffer_size: args.buffer_size,
        #[cfg(feature = "hash")]
        hash: args.hash,
        max_files: 0,
        overwrite: false,
        ignore: None,
        recursive: false,
        #[cfg(feature = "inotify")]
        watch: false,
        tls: args.tls,
    };

    if let Err(e) = lidi_clients::file::send::send_files(&config, args.files, None) {
        log::error!("{e}");
    }
}
