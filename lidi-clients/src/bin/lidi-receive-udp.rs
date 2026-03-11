use clap::Parser;
use std::{net, path};

#[derive(clap::Args)]
#[group(required = true, multiple = false)]
#[allow(clippy::struct_field_names)]
struct Listeners {
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port to accept TCP connections from lidi-receive"
    )]
    from_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port to accept TLS connections from lidi-receive"
    )]
    from_tls: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path of Unix socket to accept Unix connections from lidi-receive"
    )]
    from_unix: Option<path::PathBuf>,
}

#[derive(Parser)]
#[clap(about = "Receive UDP packets sent by lidi-send-udp.")]
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
        value_name = "ip:port",
        long,
        help = "IP address and port to send UDP packets from"
    )]
    to_bind: net::SocketAddr,
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port to send UDP packets to"
    )]
    to: net::SocketAddr,
    #[clap(flatten)]
    tls: lidi_clients::Tls,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = lidi_clients::init_logger(args.log_level) {
        eprintln!("failed to initialize logger: {e}");
        return;
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let diode = lidi_clients::DiodeReceive {
        from_tcp: args.from.from_tcp,
        from_tls: args.from.from_tls,
        from_unix: args.from.from_unix,
    };

    let config = lidi_clients::udp::Config {
        diode,
        buffer_size: u16::MAX as usize,
        tls: args.tls,
    };

    if let Err(e) = lidi_clients::udp::receive::receive(&config, args.to_bind, args.to) {
        log::error!("{e}");
    }
}
