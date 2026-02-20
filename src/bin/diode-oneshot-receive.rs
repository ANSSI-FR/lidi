use clap::Parser;
use diode::{protocol, receive};
use std::{io, net, process, str::FromStr, thread, time};

fn parse_duration_seconds(input: &str) -> Result<time::Duration, <u64 as FromStr>::Err> {
    let input = input.parse()?;
    Ok(time::Duration::from_secs(input))
}

#[derive(Parser)]
#[clap(
    about = "Receive data from diode-oneshot-send and write them to stdout (no need for diode-send nor diode-receive)."
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
        value_name = "ip:port",
        long,
        help = "IP address where to receive UDP packets from diode-send"
    )]
    from: net::IpAddr,
    #[clap(
        value_name = "port[,port]*",
        value_delimiter = ',',
        num_args = 1..=10,
        required = true,
        long,
        help = "Ports on which to receive UDP packets from diode-send"
    )]
    from_ports: Vec<u16>,
    #[clap(
        default_value = "1500",
        value_name = "nb_bytes",
        long,
        help = "MTU of the input UDP link"
    )]
    from_mtu: u16,
    #[clap(
        value_name = "2..1024",
        long,
        help = "Use recvmmsg to receive from 2 to 1024 UDP datagrams at once"
    )]
    batch: Option<u32>,
    #[clap(
        default_value = "2",
        value_name = "seconds",
        value_parser = parse_duration_seconds,
        long,
        help = "Reset diode if no data are received after duration")]
    reset_timeout: time::Duration,
    #[clap(long, help = "Flush immediately data to clients")]
    flush: bool,
    #[clap(
        value_name = "seconds",
        value_parser = parse_duration_seconds,
        long,
        help = "Abort connections if no data received after duration (0 = no abort)")]
    abort_timeout: Option<time::Duration>,
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
    #[clap(
        default_value = "1",
        value_name = "percentage",
        long,
        help = "Minimal percentage of RaptorQ repair data required before decoding"
    )]
    min_repair: u32,
    #[clap(long, help = "Hash each client transfered data")]
    hash: bool,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = diode::init_logger(args.log_level, None, true) {
        eprintln!("failed to initialize logger: {e}");
        return;
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let raptorq =
        match protocol::RaptorQ::new(args.from_mtu, args.block, args.repair, args.min_repair) {
            Ok(raptorq) => raptorq,
            Err(e) => {
                log::error!("{e}");
                return;
            }
        };

    let receiver = match receive::Receiver::new(
        receive::Config {
            from: args.from,
            from_ports: args.from_ports,
            from_mtu: args.from_mtu,
            max_clients: 1,
            flush: args.flush,
            reset_timeout: args.reset_timeout,
            abort_timeout: args.abort_timeout,
            heartbeat_interval: None,
            batch_receive: args.batch,
            hash: args.hash,
        },
        raptorq,
        |_| Ok::<_, io::Error>(io::stdout()),
        |_, ok| {
            if ok {
                process::exit(0);
            } else {
                process::exit(1);
            }
        },
    ) {
        Ok(receiver) => receiver,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    thread::scope(|scope| {
        if let Err(e) = receiver.start(scope) {
            log::error!("failed to start diode receiver: {e}");
        }
    });
}
