use std::{fmt, fs, path};

#[cfg(not(any(feature = "send-native", feature = "send-msg", feature = "send-mmsg")))]
compile_error!(
    "at least one of the following feature is required: \"send-native\", \"send-msg\", \"send-mmsg\""
);

#[cfg(not(any(
    feature = "receive-native",
    feature = "receive-msg",
    feature = "receive-mmsg"
)))]
compile_error!(
    "at least one of the following feature is required: \"receive-native\", \"receive-msg\", \"receive-mmsg\""
);

pub mod aux;
pub mod protocol;
pub mod receive;
pub mod send;
// Allow unsafe code to call libc function setsockopt.
#[allow(unsafe_code)]
mod sock_utils;
// Allow unsafe code to initialize C structs and call
// libc functions recv_mmsg and send_mmsg.
#[allow(unsafe_code)]
mod udp;

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum RecvMode {
    #[cfg(feature = "receive-native")]
    Native,
    #[cfg(feature = "receive-msg")]
    Recvmsg,
    #[cfg(feature = "receive-mmsg")]
    Recvmmsg,
}

impl Default for RecvMode {
    fn default() -> Self {
        let options = [
            #[cfg(feature = "receive-native")]
            Self::Native,
            #[cfg(feature = "receive-msg")]
            Self::Recvmsg,
            #[cfg(feature = "receive-mmsg")]
            Self::Recvmmsg,
        ];
        options[0]
    }
}

impl fmt::Display for RecvMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            #[cfg(feature = "receive-native")]
            Self::Native => write!(f, "native"),
            #[cfg(feature = "receive-msg")]
            Self::Recvmsg => write!(f, "recvmsg"),
            #[cfg(feature = "receive-mmsg")]
            Self::Recvmmsg => write!(f, "recvmmsg"),
        }
    }
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum SendMode {
    #[cfg(feature = "send-native")]
    Native,
    #[cfg(feature = "send-msg")]
    Sendmsg,
    #[cfg(feature = "send-mmsg")]
    Sendmmsg,
}

impl Default for SendMode {
    fn default() -> Self {
        let options = [
            #[cfg(feature = "send-native")]
            Self::Native,
            #[cfg(feature = "send-msg")]
            Self::Sendmsg,
            #[cfg(feature = "send-mmsg")]
            Self::Sendmmsg,
        ];
        options[0]
    }
}

impl fmt::Display for SendMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            #[cfg(feature = "send-native")]
            Self::Native => write!(f, "native"),
            #[cfg(feature = "send-msg")]
            Self::Sendmsg => write!(f, "sendmsg"),
            #[cfg(feature = "send-mmsg")]
            Self::Sendmmsg => write!(f, "sendmmsg"),
        }
    }
}

/// # Errors
///
/// Will return `Err` if `file` cannot be opened
/// or logger cannot be set (Term or file mode).
pub fn init_logger(
    level_filter: log::LevelFilter,
    file: Option<path::PathBuf>,
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

    match file {
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
