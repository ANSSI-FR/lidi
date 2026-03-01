use crate::config;
use clap::Parser;
use std::{net, path, str::FromStr};

fn endpoint_parser(args: &str) -> Result<config::Endpoint, String> {
    match args.split_once(':') {
        Some(("tcp", sockaddr)) => {
            let sockaddr = net::SocketAddr::from_str(sockaddr)
                .map_err(|e| format!("invalid socket addr for tcp endpoint: {e}"))?;
            Ok(config::Endpoint::Tcp(sockaddr))
        }
        Some(("unix", path)) => Ok(config::Endpoint::Unix(path::PathBuf::from(path))),
        Some((prefix, _)) => Err(format!("unsupported prefix {prefix:?}")),
        None => Err(String::from(
            "invalid endpoint: missing prefix tcp: or unix:",
        )),
    }
}

#[derive(Parser)]
struct CommonArgs {
    #[clap(
        help = "Path to configuration file (will be read before applying command line arguments)"
    )]
    config_file: Option<path::PathBuf>,
    #[clap(long, help = "MTU of the link between sender and receiver")]
    mtu: Option<u16>,
    #[clap(
        long,
        help = "Ports for UDP communications between sender and receiver",
        value_delimiter = ','
    )]
    ports: Option<Vec<u16>>,
    #[clap(long, help = "Size in bytes of RaptorQ block")]
    block: Option<u32>,
    #[clap(long, help = "Number of repair RaptorQ packets")]
    repair: Option<u16>,
    #[clap(long, help = "Maximal number of simultaneous clients connections")]
    max_clients: Option<u32>,
    #[clap(long, help = "Compute hash of data transmitted for each client")]
    hash: Option<bool>,
    #[clap(long, help = "Flush immediately data transmitted by each client")]
    flush: Option<bool>,
    #[clap(
        long,
        help = "Duration in seconds between sent/expected heartbeat message (0 to disable)"
    )]
    heartbeat: Option<u64>,
}

impl TryFrom<CommonArgs> for config::Config {
    type Error = crate::Error;

    fn try_from(args: CommonArgs) -> Result<Self, Self::Error> {
        let mut config = if let Some(file) = args.config_file {
            config::parse(file)?
        } else {
            Self::default()
        };

        if let Some(mtu) = args.mtu {
            config.common_mut().mtu = Some(mtu);
        }

        if let Some(ports) = args.ports {
            config.common_mut().ports = ports;
        }

        if let Some(block) = args.block {
            config.common_mut().block = Some(block);
        }

        if let Some(repair) = args.repair {
            config.common_mut().repair = Some(repair);
        }

        if let Some(max_clients) = args.max_clients {
            config.common_mut().max_clients = Some(max_clients);
        }

        if let Some(hash) = args.hash {
            config.common_mut().hash = Some(hash);
        }

        if let Some(flush) = args.flush {
            config.common_mut().flush = Some(flush);
        }

        if let Some(heartbeat) = args.heartbeat {
            config.common_mut().heartbeat = Some(heartbeat);
        }

        Ok(config)
    }
}

pub trait Args: clap::Parser + TryInto<config::Config> {
    fn parse_command_line() -> Result<config::Config, <Self as TryInto<config::Config>>::Error> {
        Self::parse().try_into()
    }
}

#[derive(Parser)]
pub struct SendArgs {
    #[clap(flatten)]
    common: CommonArgs,
    #[clap(long, help = "Log level")]
    log: Option<log::LevelFilter>,
    #[clap(long, help = "Log file")]
    log_file: Option<path::PathBuf>,
    #[clap(long, help = "Add a client endpoint (tcp:<IP:PORT> or unix:<PATH>)", value_parser = endpoint_parser)]
    from: Option<Vec<config::Endpoint>>,
    #[clap(long, help = "IP address of receiver")]
    to: Option<net::IpAddr>,
    #[clap(long, help = "Binding IP:port of UDP socket used to reach reaceiver")]
    to_bind: Option<net::SocketAddr>,
    #[clap(long, help = "Mode used to send UDP packets")]
    mode: Option<config::Mode>,
}

impl Args for SendArgs {}

impl TryFrom<SendArgs> for config::Config {
    type Error = crate::Error;

    fn try_from(args: SendArgs) -> Result<Self, Self::Error> {
        let mut config: Self = args.common.try_into()?;

        if let Some(log) = args.log {
            config.send_mut().log = Some(log);
        }

        if let Some(log_file) = args.log_file {
            config.send_mut().log_file = Some(log_file);
        }

        if let Some(from) = args.from {
            config.send_mut().from = from;
        }

        if let Some(to) = args.to {
            config.send_mut().to = Some(to);
        }

        if let Some(to_bind) = args.to_bind {
            config.send_mut().to_bind = Some(to_bind);
        }

        if let Some(mode) = args.mode {
            config.send_mut().mode = Some(mode);
        }

        Ok(config)
    }
}

#[derive(Parser)]
pub struct ReceiveArgs {
    #[clap(flatten)]
    common: CommonArgs,
    #[clap(long, help = "Log level")]
    log: Option<log::LevelFilter>,
    #[clap(long, help = "Log file")]
    log_file: Option<path::PathBuf>,
    #[clap(long, help = "Add a client endpoint (tcp:<IP:PORT> or unix:<PATH>)", value_parser = endpoint_parser)]
    to: Option<Vec<config::Endpoint>>,
    #[clap(long, help = "IP address on which to listen from sender UDP packets")]
    from: Option<net::IpAddr>,
    #[clap(long, help = "Mode used to receive UDP packets")]
    mode: Option<config::Mode>,
    #[clap(
        long,
        help = "Maximum number of RaptorQ blocks to buffer for each client (0 means infinite)"
    )]
    queue_size: Option<usize>,
    #[clap(
        long,
        help = "Duration in seconds without UDP packets before resetting the internal state of the RaptorQ receiver"
    )]
    reset_timeout: Option<u64>,
    #[clap(
        long,
        help = "Duration in seconds without data for a client before closing the client connection"
    )]
    abort_timeout: Option<u64>,
}

impl Args for ReceiveArgs {}

impl TryFrom<ReceiveArgs> for config::Config {
    type Error = crate::Error;

    fn try_from(args: ReceiveArgs) -> Result<Self, Self::Error> {
        let mut config: Self = args.common.try_into()?;

        if let Some(log) = args.log {
            config.receive_mut().log = Some(log);
        }

        if let Some(log_file) = args.log_file {
            config.receive_mut().log_file = Some(log_file);
        }

        if let Some(to) = args.to {
            config.receive_mut().to = to;
        }

        if let Some(from) = args.from {
            config.receive_mut().from = Some(from);
        }

        if let Some(mode) = args.mode {
            config.receive_mut().mode = Some(mode);
        }

        if let Some(queue_size) = args.queue_size {
            config.receive_mut().queue_size = Some(queue_size);
        }

        if let Some(reset_timeout) = args.reset_timeout {
            config.receive_mut().reset_timeout = Some(reset_timeout);
        }

        if let Some(abort_timeout) = args.abort_timeout {
            config.receive_mut().abort_timeout = Some(abort_timeout);
        }

        Ok(config)
    }
}
