use std::{env, fmt, path};

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
pub fn init_logger(level_filter: log::LevelFilter, stderr_only: bool) -> Result<(), String> {
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

    simplelog::TermLogger::init(
        level_filter,
        config,
        terminal_mode,
        simplelog::ColorChoice::Auto,
    )
    .map_err(|e| e.to_string())
}

pub fn command_arguments(stderr_only: bool) -> Result<config::Config, Error> {
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

    if let Err(e) = init_logger(config.send().log(), stderr_only) {
        return Err(Error::Logger(e));
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    Ok(config)
}
