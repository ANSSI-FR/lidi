use serde::Deserialize;
use std::{
    fmt, fs,
    io::{self, Read},
    net, path, time,
};

const DEFAULT_HASH: bool = false;
const DEFAULT_FLUSH: bool = false;
const DEFAULT_LOG_LEVEL: log::LevelFilter = log::LevelFilter::Info;
const DEFAULT_MAX_CLIENTS: u32 = 2;
const DEFAULT_RECEIVER: net::IpAddr = net::IpAddr::V4(net::Ipv4Addr::LOCALHOST);
const DEFAULT_MTU: u16 = 1500;
const DEFAULT_BLOCK: u32 = 200_000;
const DEFAULT_REPAIR: u16 = 2;
const DEFAULT_RESET_TIMEOUT_SECONDS: u64 = 2;
const DEFAULT_CLIENT_QUEUE_SIZE: usize = 0;

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
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Parsing(e) => write!(fmt, "parsing error: {e}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Mode {
    Native,
    Msg,
    Mmsg,
}

impl fmt::Display for Mode {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
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
    Unix(path::PathBuf),
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommonConfig {
    mtu: Option<u16>,
    ports: Vec<u16>,
    block: Option<u32>,
    repair: Option<u16>,
    max_clients: Option<u32>,
    hash: Option<bool>,
    flush: Option<bool>,
    heartbeat: Option<u64>,
}

impl CommonConfig {
    #[must_use]
    pub fn mtu(&self) -> u16 {
        self.mtu.unwrap_or(DEFAULT_MTU)
    }

    #[must_use]
    pub fn ports(&self) -> Vec<u16> {
        self.ports.clone()
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
        self.heartbeat.map(time::Duration::from_secs)
    }
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendConfig {
    log: Option<log::LevelFilter>,
    from: Vec<Endpoint>,
    to: Option<net::IpAddr>,
    to_bind: Option<net::SocketAddr>,
    mode: Option<Mode>,
}

impl SendConfig {
    #[must_use]
    pub fn log(&self) -> log::LevelFilter {
        self.log.unwrap_or(DEFAULT_LOG_LEVEL)
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
    log: Option<log::LevelFilter>,
    to: Vec<Endpoint>,
    from: Option<net::IpAddr>,
    mode: Option<Mode>,
    client_queue_size: Option<usize>,
    reset_timeout: Option<u64>,
    abort_timeout: Option<u64>,
}

impl ReceiveConfig {
    #[must_use]
    pub fn log(&self) -> log::LevelFilter {
        self.log.unwrap_or(DEFAULT_LOG_LEVEL)
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
    pub fn client_queue_size(&self) -> usize {
        self.client_queue_size.unwrap_or(DEFAULT_CLIENT_QUEUE_SIZE)
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

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    common: Option<CommonConfig>,
    send: Option<SendConfig>,
    receive: Option<ReceiveConfig>,
}

impl Config {
    pub fn set_max_clients(&mut self, max_clients: u32) {
        let mut common = self.common.take().unwrap_or_default();
        common.max_clients = Some(max_clients);
        self.common.replace(common);
    }

    pub fn set_heartbeat(&mut self, heartbeat: Option<u64>) {
        let mut common = self.common.take().unwrap_or_default();
        common.heartbeat = heartbeat;
        self.common.replace(common);
    }

    #[must_use]
    pub fn common(&self) -> CommonConfig {
        self.common.clone().unwrap_or_default()
    }

    #[must_use]
    pub fn send(&self) -> SendConfig {
        self.send.clone().unwrap_or_default()
    }

    #[must_use]
    pub fn receive(&self) -> ReceiveConfig {
        self.receive.clone().unwrap_or_default()
    }
}

pub fn parse(file: path::PathBuf) -> Result<Config, Error> {
    let mut file = fs::OpenOptions::new().read(true).write(false).open(file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(Config::deserialize(toml::Deserializer::parse(&content)?)?)
}
