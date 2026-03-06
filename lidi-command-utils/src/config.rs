use serde::Deserialize;
use std::{
    fmt, fs,
    io::{self, Read},
    net, path, time,
};

const DEFAULT_RECEIVER: net::IpAddr = net::IpAddr::V4(net::Ipv4Addr::LOCALHOST);
const DEFAULT_PORTS: &[u16] = &[5000];

const DEFAULT_HASH: bool = false;
const DEFAULT_FLUSH: bool = false;
const DEFAULT_LOG_LEVEL: log::LevelFilter = log::LevelFilter::Info;
const DEFAULT_MAX_CLIENTS: u32 = 2;
const DEFAULT_MTU: u16 = 1500;
const DEFAULT_BLOCK: u32 = 200_000;
const DEFAULT_REPAIR: u16 = 2;
const DEFAULT_RESET_TIMEOUT_SECONDS: u64 = 2;
const DEFAULT_QUEUE_SIZE: usize = 0;

#[allow(unused)]
const DEFAULT_TLS_MIN: TlsVersion = TlsVersion::Tls1_3;
#[allow(unused)]
const DEFAULT_TLS_METHOD: TlsMethod = TlsMethod::Mozilla_Modern_v5;

pub enum Error {
    Io(io::Error),
    Parsing(toml::de::Error),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Self::Parsing(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Parsing(e) => write!(fmt, "parsing error: {e}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "command-line", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Mode {
    Native,
    Msg,
    Mmsg,
}

impl fmt::Display for Mode {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Native => write!(fmt, "native"),
            Self::Msg => write!(fmt, "msg"),
            Self::Mmsg => write!(fmt, "mmsg"),
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Endpoint {
    Tcp(net::SocketAddr),
    Tls(net::SocketAddr),
    Unix(path::PathBuf),
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommonConfig {
    pub(crate) mtu: Option<u16>,
    pub(crate) ports: Option<Vec<u16>>,
    pub(crate) block: Option<u32>,
    pub(crate) repair: Option<u16>,
    pub max_clients: Option<u32>,
    pub(crate) hash: Option<bool>,
    pub(crate) flush: Option<bool>,
    pub heartbeat: Option<u64>,
}

impl CommonConfig {
    #[must_use]
    pub fn mtu(&self) -> u16 {
        self.mtu.unwrap_or(DEFAULT_MTU)
    }

    #[must_use]
    pub fn ports(&self) -> Vec<u16> {
        self.ports
            .clone()
            .unwrap_or_else(|| Vec::from(DEFAULT_PORTS))
    }

    #[must_use]
    pub fn block(&self) -> u32 {
        self.block.unwrap_or(DEFAULT_BLOCK)
    }

    #[must_use]
    pub fn repair(&self) -> u16 {
        self.repair.unwrap_or(DEFAULT_REPAIR)
    }

    #[must_use]
    pub fn max_clients(&self) -> u32 {
        self.max_clients.unwrap_or(DEFAULT_MAX_CLIENTS)
    }

    #[must_use]
    pub fn hash(&self) -> bool {
        self.hash.unwrap_or(DEFAULT_HASH)
    }

    #[must_use]
    pub fn flush(&self) -> bool {
        self.flush.unwrap_or(DEFAULT_FLUSH)
    }

    #[must_use]
    pub fn heartbeat(&self) -> Option<time::Duration> {
        self.heartbeat
            .filter(|heartbeat| 0 < *heartbeat)
            .map(time::Duration::from_secs)
    }
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum TlsVersion {
    Tls1_1,
    Tls1_2,
    Tls1_3,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
#[allow(non_camel_case_types)]
pub enum TlsMethod {
    Mozilla_Intermediate_v4,
    Mozilla_Intermediate_v5,
    Mozilla_Modern_v4,
    Mozilla_Modern_v5,
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    key: Option<path::PathBuf>,
    certificate: Option<path::PathBuf>,
    ca: Option<path::PathBuf>,
    tls_min: Option<TlsVersion>,
    tls_method: Option<TlsMethod>,
    ciphers: Option<String>,
    groups: Option<String>,
}

#[allow(unused)]
impl TlsConfig {
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

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendConfig {
    pub(crate) log: Option<log::LevelFilter>,
    pub(crate) log_file: Option<path::PathBuf>,
    tls: Option<TlsConfig>,
    pub(crate) prometheus_listen: Option<net::SocketAddr>,
    pub(crate) from: Vec<Endpoint>,
    pub(crate) to: Option<net::IpAddr>,
    pub(crate) to_bind: Option<net::SocketAddr>,
    pub(crate) mode: Option<Mode>,
}

impl SendConfig {
    #[must_use]
    pub(crate) fn log(&self) -> log::LevelFilter {
        self.log.unwrap_or(DEFAULT_LOG_LEVEL)
    }

    #[must_use]
    pub(crate) fn log_file(&self) -> Option<path::PathBuf> {
        self.log_file.clone()
    }

    #[must_use]
    pub fn tls(&self) -> TlsConfig {
        self.tls.clone().unwrap_or_default()
    }

    #[must_use]
    pub const fn prometheus_listen(&self) -> Option<net::SocketAddr> {
        self.prometheus_listen
    }

    #[must_use]
    pub fn from(&self) -> Vec<Endpoint> {
        self.from.clone()
    }

    #[must_use]
    pub fn to(&self) -> net::IpAddr {
        self.to.unwrap_or(DEFAULT_RECEIVER)
    }

    #[must_use]
    pub fn to_bind(&self) -> net::SocketAddr {
        let ip4 = net::Ipv4Addr::UNSPECIFIED;
        self.to_bind
            .unwrap_or_else(|| net::SocketAddr::new(net::IpAddr::V4(ip4), 0))
    }

    #[must_use]
    pub const fn mode(&self) -> Option<Mode> {
        self.mode
    }
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiveConfig {
    pub(crate) log: Option<log::LevelFilter>,
    pub(crate) log_file: Option<path::PathBuf>,
    tls: Option<TlsConfig>,
    pub(crate) prometheus_listen: Option<net::SocketAddr>,
    pub(crate) to: Vec<Endpoint>,
    pub(crate) from: Option<net::IpAddr>,
    pub(crate) mode: Option<Mode>,
    pub(crate) queue_size: Option<usize>,
    pub(crate) reset_timeout: Option<u64>,
    pub(crate) abort_timeout: Option<u64>,
}

impl ReceiveConfig {
    #[must_use]
    pub(crate) fn log(&self) -> log::LevelFilter {
        self.log.unwrap_or(DEFAULT_LOG_LEVEL)
    }

    #[must_use]
    pub(crate) fn log_file(&self) -> Option<path::PathBuf> {
        self.log_file.clone()
    }

    #[must_use]
    pub fn tls(&self) -> TlsConfig {
        self.tls.clone().unwrap_or_default()
    }

    #[must_use]
    pub const fn prometheus_listen(&self) -> Option<net::SocketAddr> {
        self.prometheus_listen
    }

    #[must_use]
    pub fn to(&self) -> Vec<Endpoint> {
        self.to.clone()
    }

    #[must_use]
    pub fn from(&self) -> net::IpAddr {
        self.from.unwrap_or(DEFAULT_RECEIVER)
    }

    #[must_use]
    pub const fn mode(&self) -> Option<Mode> {
        self.mode
    }

    #[must_use]
    pub fn queue_size(&self) -> usize {
        self.queue_size.unwrap_or(DEFAULT_QUEUE_SIZE)
    }

    #[must_use]
    pub fn reset_timeout(&self) -> time::Duration {
        time::Duration::from_secs(self.reset_timeout.unwrap_or(DEFAULT_RESET_TIMEOUT_SECONDS))
    }

    #[must_use]
    pub fn abort_timeout(&self) -> Option<time::Duration> {
        self.abort_timeout.map(time::Duration::from_secs)
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(flatten)]
    common: Option<CommonConfig>,
    pub(crate) send: Option<SendConfig>,
    pub(crate) receive: Option<ReceiveConfig>,
}

impl Config {
    #[must_use]
    pub fn common(&self) -> CommonConfig {
        self.common.clone().unwrap_or_default()
    }

    #[allow(clippy::missing_panics_doc)] // cannot panic
    pub fn common_mut(&mut self) -> &mut CommonConfig {
        if self.common.is_none() {
            self.common = Some(CommonConfig::default());
        }
        self.common.as_mut().unwrap()
    }

    #[must_use]
    pub fn send(&self) -> SendConfig {
        self.send.clone().unwrap_or_default()
    }

    #[allow(clippy::missing_panics_doc)] // cannot panic
    pub fn send_mut(&mut self) -> &mut SendConfig {
        if self.send.is_none() {
            self.send = Some(SendConfig::default());
        }
        self.send.as_mut().unwrap()
    }

    #[must_use]
    pub fn receive(&self) -> ReceiveConfig {
        self.receive.clone().unwrap_or_default()
    }

    #[allow(clippy::missing_panics_doc)] // cannot panic
    pub fn receive_mut(&mut self) -> &mut ReceiveConfig {
        if self.receive.is_none() {
            self.receive = Some(ReceiveConfig::default());
        }
        self.receive.as_mut().unwrap()
    }
}

pub(crate) fn parse(file: path::PathBuf) -> Result<Config, Error> {
    let mut file = fs::OpenOptions::new().read(true).write(false).open(file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(Config::deserialize(toml::Deserializer::parse(&content)?)?)
}
