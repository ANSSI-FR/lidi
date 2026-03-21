#[cfg(feature = "command-line")]
use clap::Parser;
use std::{env, fmt, net, path};

pub mod config;
#[cfg(feature = "hash")]
pub mod hash;
pub mod socket;
#[cfg(feature = "tls")]
pub mod tls;

pub enum Error {
    Arguments(String),
    Config(config::Error),
    Logger(String),
    #[cfg(feature = "tls")]
    Tls(tls::Error),
    #[cfg(feature = "prometheus")]
    Prometheus(metrics_exporter_prometheus::BuildError),
}

impl From<config::Error> for Error {
    fn from(e: config::Error) -> Self {
        Self::Config(e)
    }
}

#[cfg(feature = "tls")]
impl From<tls::Error> for Error {
    fn from(e: tls::Error) -> Self {
        Self::Tls(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Arguments(e) => write!(fmt, "argument(s) error: {e}"),
            Self::Config(e) => write!(fmt, "configuration error: {e}"),
            Self::Logger(e) => write!(fmt, "logger error: {e}"),
            #[cfg(feature = "tls")]
            Self::Tls(e) => write!(fmt, "TLS error: {e}"),
            #[cfg(feature = "prometheus")]
            Self::Prometheus(e) => write!(fmt, "Prometheus error: {e}"),
        }
    }
}

fn init_logger_simplelog(level_filter: log::LevelFilter, stderr_only: bool) -> Result<(), Error> {
    let config = simplelog::ConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Right)
        .set_target_level(simplelog::LevelFilter::Off)
        .set_thread_level(level_filter)
        .set_thread_mode(simplelog::ThreadLogMode::Names)
        .set_time_format_rfc2822()
        .set_time_offset_to_local()
        .unwrap_or_else(|e| e)
        .build();

    let terminal = if stderr_only {
        simplelog::TerminalMode::Stderr
    } else {
        simplelog::TerminalMode::Mixed
    };

    simplelog::TermLogger::init(level_filter, config, terminal, simplelog::ColorChoice::Auto)
        .map_err(|e| Error::Logger(format!("failed to initialize simplelog: {e}")))
}

fn init_logger(
    level_filter: log::LevelFilter,
    log4rs_config: Option<&path::PathBuf>,
    stderr_only: bool,
) -> Result<(), Error> {
    #[cfg(not(feature = "log4rs"))]
    {
        if log4rs_config.is_some() {
            eprintln!("log4rs configuration is enabled, but log4rs was not enabled at compilation");
        }
        init_logger_simplelog(level_filter, stderr_only)
    }

    #[cfg(feature = "log4rs")]
    log4rs_config.map_or_else(
        || init_logger_simplelog(level_filter, stderr_only),
        |log4rs_config| {
            log4rs::config::init_file(log4rs_config, log4rs::config::Deserializers::default())
                .map_err(|e| {
                    Error::Logger(format!(
                        "failed to configure log4rs with {}: {e}",
                        log4rs_config.display()
                    ))
                })
        },
    )
}

#[allow(clippy::unnecessary_wraps)]
fn init_prometheus(prometheus_listen: Option<net::SocketAddr>) -> Result<(), Error> {
    #[allow(unused)]
    if let Some(prometheus_listen) = prometheus_listen {
        #[cfg(not(feature = "prometheus"))]
        log::warn!("Prometheus is configured, but Prometheus was not enabled at compilation");

        #[cfg(feature = "prometheus")]
        metrics_exporter_prometheus::PrometheusBuilder::new()
            .with_http_listener(prometheus_listen)
            .install()
            .map_err(Error::Prometheus)?;
    }

    Ok(())
}

#[derive(Clone, Copy)]
pub enum Role {
    Receive,
    Send,
}

impl Role {
    #[cfg(feature = "command-line")]
    fn parse_command_line(self) -> Result<config::Config, Error> {
        let config = match self {
            Self::Receive => {
                let mut receive_config = config::ReceiveConfig::parse();
                if let Some(config_file) = receive_config.common.config_file {
                    let mut config = config::ReceiveConfig::from(config::parse(config_file)?);
                    config.update_from(env::args());
                    receive_config = config;
                }

                config::Config {
                    common: receive_config.common,
                    receive: receive_config.receive,
                    ..Default::default()
                }
            }
            Self::Send => {
                let mut send_config = config::SendConfig::parse();
                if let Some(config_file) = send_config.common.config_file {
                    let mut config = config::SendConfig::from(config::parse(config_file)?);
                    config.update_from(env::args());
                    send_config = config;
                }

                config::Config {
                    common: send_config.common,
                    send: send_config.send,
                    ..Default::default()
                }
            }
        };

        Ok(config)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Receive => write!(fmt, "receive"),
            Self::Send => write!(fmt, "send"),
        }
    }
}

#[cfg(not(feature = "command-line"))]
fn no_command_line() -> Result<config::Config, Error> {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.len() > 1 {
        return Err(Error::Arguments(String::from(
            "too many arguments: expecting only configuration file",
        )));
    }

    let Some(file) = args.first() else {
        return Err(Error::Arguments(String::from(
            "missing argument: <config_file>",
        )));
    };

    let config = config::parse(path::PathBuf::from(file))?;

    Ok(config)
}

#[allow(unused)]
pub fn command_arguments(
    role: Role,
    stderr_only: bool,
    tls_init: bool,
    prometheus_init: bool,
) -> Result<config::Config, Error> {
    #[cfg(not(feature = "command-line"))]
    let config = no_command_line()?;

    #[cfg(feature = "command-line")]
    let config = role.parse_command_line()?;

    let (log, log_file, prometheus_listen) = match role {
        Role::Send => (
            config.send.log(),
            config.send.log4rs_config(),
            config.send.prometheus_listen(),
        ),
        Role::Receive => (
            config.receive.log(),
            config.receive.log4rs_config(),
            config.receive.prometheus_listen(),
        ),
    };

    init_logger(log, log_file.as_ref(), stderr_only)?;

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    #[cfg(feature = "tls")]
    if tls_init {
        tls::init();
    }

    if prometheus_init {
        init_prometheus(prometheus_listen)?;
    }

    Ok(config)
}
