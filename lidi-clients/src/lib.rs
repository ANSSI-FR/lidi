pub mod file;
#[cfg(feature = "hash")]
pub(crate) mod hash;
pub mod udp;

use std::{fmt, net, path};

pub enum DiodeSend {
    Tcp(net::SocketAddr),
    Unix(path::PathBuf),
}

impl fmt::Display for DiodeSend {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Tcp(s) => write!(fmt, "TCP {s}"),
            Self::Unix(p) => write!(fmt, "Unix {}", p.display()),
        }
    }
}

pub struct DiodeReceive {
    pub from_tcp: Option<net::SocketAddr>,
    pub from_unix: Option<path::PathBuf>,
}

impl fmt::Display for DiodeReceive {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(from_tcp) = &self.from_tcp {
            write!(fmt, "TCP {from_tcp}")?;
        }
        if let Some(from_unix) = &self.from_unix {
            write!(fmt, "Unix {}", from_unix.display())?;
        }
        Ok(())
    }
}

/// # Errors
///
/// Will return `Err` if `file` cannot be opened
/// or logger cannot be set (Term or file mode).
pub fn init_logger(level_filter: log::LevelFilter) -> Result<(), String> {
    let terminal_mode = simplelog::TerminalMode::Mixed;

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
