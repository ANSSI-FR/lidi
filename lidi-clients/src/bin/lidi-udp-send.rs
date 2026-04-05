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
#[clap(about = "Send UDP datagrams to lidi-udp-receive.")]
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
        value_name = "ip:port",
        long,
        help = "IP address and port to receive UDP packets"
    )]
    from: net::SocketAddr,
    #[clap(flatten)]
    tls: lidi_clients::Tls,
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

    let config = lidi_clients::udp::Config {
        diode,
        buffer_size: u16::MAX as usize,
        tls: args.tls,
    };

    if let Err(e) = lidi_clients::udp::send::send(&config, args.from) {
        log::error!("{e}");
    }
}
