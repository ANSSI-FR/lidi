use std::{fmt, net, path};

pub mod file;
#[cfg(feature = "hash")]
pub(crate) mod hash;
#[cfg(feature = "tls")]
pub mod tls;
pub mod udp;

#[derive(Clone, Copy, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum TlsVersion {
    Tls1_1,
    Tls1_2,
    Tls1_3,
}

#[derive(Clone, Copy, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
#[allow(non_camel_case_types)]
pub enum TlsMethod {
    Mozilla_Intermediate_v4,
    Mozilla_Intermediate_v5,
    Mozilla_Modern_v4,
    Mozilla_Modern_v5,
}

#[derive(Clone, Default, clap::Parser)]
#[allow(clippy::struct_field_names)]
pub struct Tls {
    #[clap(value_name = "path", long = "tls-key", help = "Path to PEM key file")]
    key: Option<path::PathBuf>,
    #[clap(
        value_name = "path",
        long = "tls-certificate",
        help = "Path to PEM certificate file"
    )]
    certificate: Option<path::PathBuf>,
    #[clap(
        value_name = "path",
        long = "tls-ca",
        help = "Path to PEM accepted CA file"
    )]
    ca: Option<path::PathBuf>,
    #[clap(long = "tls-min", help = "Minimum TLS accepted version")]
    tls_min: Option<TlsVersion>,
    #[clap(long = "tls-method", help = "Minimum TLS accepted method")]
    tls_method: Option<TlsMethod>,
    #[clap(long = "tls-ciphers", help = "Accepted TLS cipers")]
    ciphers: Option<String>,
    #[clap(long = "tls-groups", help = "Accepted TLS groups")]
    groups: Option<String>,
}

#[allow(unused)]
const DEFAULT_TLS_MIN: TlsVersion = TlsVersion::Tls1_3;
#[allow(unused)]
const DEFAULT_TLS_METHOD: TlsMethod = TlsMethod::Mozilla_Modern_v5;

#[allow(unused)]
impl Tls {
    #[must_use]
    pub(crate) const fn key(&self) -> Option<&path::PathBuf> {
        self.key.as_ref()
    }

    #[must_use]
    pub(crate) const fn certificate(&self) -> Option<&path::PathBuf> {
        self.certificate.as_ref()
    }

    #[must_use]
    pub(crate) const fn ca(&self) -> Option<&path::PathBuf> {
        self.ca.as_ref()
    }

    #[must_use]
    pub(crate) const fn ciphers(&self) -> Option<&String> {
        self.ciphers.as_ref()
    }

    #[must_use]
    pub(crate) const fn groups(&self) -> Option<&String> {
        self.groups.as_ref()
    }

    #[must_use]
    pub(crate) fn tls_min(&self) -> TlsVersion {
        self.tls_min.unwrap_or(DEFAULT_TLS_MIN)
    }

    #[must_use]
    pub(crate) fn tls_method(&self) -> TlsMethod {
        self.tls_method.unwrap_or(DEFAULT_TLS_METHOD)
    }
}

pub enum DiodeSend {
    Tcp(net::SocketAddr),
    Tls(net::SocketAddr),
    Unix(path::PathBuf),
}

impl fmt::Display for DiodeSend {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Tcp(s) => write!(fmt, "TCP {s}"),
            Self::Tls(s) => write!(fmt, "TLS {s}"),
            Self::Unix(p) => write!(fmt, "Unix {}", p.display()),
        }
    }
}

pub struct DiodeReceive {
    pub from_tcp: Option<net::SocketAddr>,
    pub from_tls: Option<net::SocketAddr>,
    pub from_unix: Option<path::PathBuf>,
}

impl fmt::Display for DiodeReceive {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(from_tcp) = &self.from_tcp {
            write!(fmt, "TCP {from_tcp}")?;
        }
        if let Some(from_tls) = &self.from_tls {
            write!(fmt, "TLS {from_tls}")?;
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
