use clap::Parser;
use diode::{protocol, send};
use std::{io, net, process, sync, thread};

#[derive(clap::Parser)]
#[clap(
    about = "Read stdin and send it to diode-oneshot-receive (no need for diode-send nor diode-receive)."
)]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Off|Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(
        default_value = "1",
        value_name = "0..255",
        long,
        help = "Number of parallel RaptorQ encoding threads"
    )]
    encode_threads: u8,
    #[clap(long, help = "Flush client data immediately")]
    flush: bool,
    #[clap(
        value_name = "ip:port",
        long,
        help = "IP address and port where to send UDP packets to diode-receive"
    )]
    to: net::SocketAddr,
    #[clap(
        default_value = "0.0.0.0:0",
        value_name = "ip:port",
        long,
        help = "Binding IP for UDP traffic"
    )]
    to_bind: net::SocketAddr,
    #[clap(
        default_value = "1500",
        value_name = "nb_bytes",
        long,
        help = "MTU of the output UDP link"
    )]
    to_mtu: u16,
    #[clap(
        value_name = "2..1024",
        long,
        help = "Use sendmmsg to send from 2 to 1024 UDP datagrams at once"
    )]
    batch: Option<u32>,
    #[clap(
        default_value = "734928",
        value_name = "nb_bytes",
        long,
        help = "Size of RaptorQ block in bytes"
    )]
    block: u32,
    #[clap(
        default_value = "2",
        value_name = "percentage",
        long,
        help = "Percentage of RaptorQ repair data"
    )]
    repair: u32,
    #[clap(long, help = "Set CPU affinity for threads")]
    cpu_affinity: bool,
}

fn main() {
    let args = Args::parse();

    diode::init_logger(args.log_level, false);

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let raptorq = match protocol::RaptorQ::new(args.to_mtu, args.block, args.repair) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let sender = match send::Sender::new(
        send::Config {
            max_clients: 1,
            flush: args.flush,
            nb_encode_threads: args.encode_threads,
            heartbeat_interval: None,
            to: args.to,
            to_bind: args.to_bind,
            to_mtu: args.to_mtu,
            batch_send: args.batch,
            cpu_affinity: args.cpu_affinity,
        },
        raptorq,
    ) {
        Ok(sender) => sender,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let sender = sync::Arc::new(sender);

    thread::scope(|scope| {
        if let Err(e) = sender.start(scope) {
            log::error!("failed to start diode sender: {e}");
        }

        let mut err = None;

        if let Err(e) = sender.new_client(io::stdin()) {
            log::error!("failed to send Unix client to connect queue: {e}");
            err = Some(1);
        }

        if let Err(e) = sender.stop() {
            log::error!("failed to send stop: {e}");
        }

        if let Some(err) = err {
            process::exit(err);
        }
    });
}
