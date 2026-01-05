use clap::Parser;
use diode::aux;
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
#[clap(about = "Send UDP datagrams to diode-receive-udp.")]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Off|Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(flatten)]
    to: Clients,
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port to receive UDP packets"
    )]
    from: net::SocketAddr,
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

    let diode = if let Some(to_tcp) = args.to.to_tcp {
        aux::DiodeSend::Tcp(to_tcp)
    } else if let Some(to_unix) = args.to.to_unix {
        aux::DiodeSend::Unix(to_unix)
    } else {
        unreachable!()
    };

    let config = aux::udp::Config {
        diode,
        buffer_size: u16::MAX as usize,
    };

    if let Err(e) = aux::udp::send::send(&config, args.from) {
        log::error!("{e}");
    }
}
