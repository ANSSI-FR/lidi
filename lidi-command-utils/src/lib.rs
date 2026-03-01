#[cfg(feature = "command-line")]
use command_line::Args;
use std::{env, fmt, fs, path};

#[cfg(feature = "command-line")]
mod command_line;
pub mod config;
#[cfg(feature = "hash")]
pub mod hash;
pub mod socket;

pub enum Error {
    Arguments(String),
    Config(config::Error),
    Logger(String),
}

impl From<config::Error> for Error {
    fn from(e: config::Error) -> Self {
        Self::Config(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Arguments(e) => write!(fmt, "argument(s) error: {e}"),
            Self::Config(e) => write!(fmt, "configuration error: {e}"),
            Self::Logger(e) => write!(fmt, "logger error: {e}"),
        }
    }
}

/// # Errors
///
/// Will return `Err` if `file` cannot be opened
/// or logger cannot be set (Term or file mode).
pub fn init_logger(
    level_filter: log::LevelFilter,
    log_file: Option<path::PathBuf>,
    stderr_only: bool,
) -> Result<(), String> {
    let terminal_mode = if stderr_only {
        simplelog::TerminalMode::Stderr
    } else {
        simplelog::TerminalMode::Mixed
    };

    let config = simplelog::ConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Right)
        .set_target_level(simplelog::LevelFilter::Off)
        .set_thread_level(level_filter)
        .set_thread_mode(simplelog::ThreadLogMode::Names)
        .set_time_format_rfc2822()
        .set_time_offset_to_local()
        .unwrap_or_else(|e| e)
        .build();

    match log_file {
        Some(file) => fs::OpenOptions::new()
            .create(true)
            .append(true)
            .truncate(false)
            .read(false)
            .open(file)
            .map_err(|e| e.to_string())
            .and_then(|file| {
                simplelog::WriteLogger::init(level_filter, config, file).map_err(|e| e.to_string())
            }),
        None => simplelog::TermLogger::init(
            level_filter,
            config,
            terminal_mode,
            simplelog::ColorChoice::Auto,
        )
        .map_err(|e| e.to_string()),
    }
}

#[derive(Clone, Copy)]
pub enum Role {
    Send,
    Receive,
}

impl Role {
    #[cfg(feature = "command-line")]
    fn parse_command_line(self) -> Result<config::Config, Error> {
        match self {
            Self::Send => command_line::SendArgs::parse_command_line(),
            Self::Receive => command_line::ReceiveArgs::parse_command_line(),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Send => write!(fmt, "send"),
            Self::Receive => write!(fmt, "receive"),
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
pub fn command_arguments(role: Role, stderr_only: bool) -> Result<config::Config, Error> {
    #[cfg(not(feature = "command-line"))]
    let config = no_command_line()?;

    #[cfg(feature = "command-line")]
    let config = role.parse_command_line()?;

    let (log, log_file) = match role {
        Role::Send => (config.send().log(), config.send().log_file()),
        Role::Receive => (config.receive().log(), config.receive().log_file()),
    };

    if let Err(e) = init_logger(log, log_file, stderr_only) {
        return Err(Error::Logger(e));
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    Ok(config)
}
