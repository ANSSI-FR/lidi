use serde::Deserialize;
use std::{
    error, fmt, fs,
    io::{self, Read},
    net, path,
    str::FromStr,
    time,
};

const DEFAULT_RECEIVER: net::IpAddr = net::IpAddr::V4(net::Ipv4Addr::LOCALHOST);
const DEFAULT_PORTS: &[u16] = &[5000];

const DEFAULT_LOG_LEVEL: log::LevelFilter = log::LevelFilter::Info;
const DEFAULT_MAX_CLIENTS: u32 = 2;
const DEFAULT_MTU: u16 = 1500;
const DEFAULT_BLOCK: u32 = 200_000;
const DEFAULT_REPAIR: u16 = 2;
const DEFAULT_RESET_TIMEOUT_SECONDS: u64 = 2;
const DEFAULT_QUEUE_SIZE: usize = 0;

#[derive(Debug)]
pub enum Error {
    Endpoint(String),
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

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Endpoint(e) => write!(fmt, "invalid endpoint description: {e}"),
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Parsing(e) => write!(fmt, "parsing error: {e}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
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

#[derive(Clone, Copy, Default)]
pub struct EndpointOptions {
    pub flush: bool,
    pub hash: bool,
}

impl fmt::Display for EndpointOptions {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(fmt, "flush = {}, hash = {}", self.flush, self.hash)
    }
}

impl FromStr for EndpointOptions {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut res = Self::default();
        for option in s.split(',') {
            let Some((name, value)) = option.split_once('=') else {
                return Err(Error::Endpoint(format!(
                    "invalid option format: {option:?}"
                )));
            };
            match name {
                "flush" => {
                    res.flush = bool::from_str(value)
                        .map_err(|e| Error::Endpoint(format!("unknown flush option value: {e}")))?;
                }
                "hash" => {
                    res.hash = bool::from_str(value)
                        .map_err(|e| Error::Endpoint(format!("unknown hash option value: {e}")))?;
                }
                n => return Err(Error::Endpoint(format!("unknown option {n:?}"))),
            }
        }
        Ok(res)
    }
}

#[derive(Clone)]
pub enum Endpoint {
    Tcp {
        address: net::SocketAddr,
        options: EndpointOptions,
    },
    Tls {
        address: net::SocketAddr,
        options: EndpointOptions,
    },
    Unix {
        path: path::PathBuf,
        options: EndpointOptions,
    },
}

impl Endpoint {
    #[must_use]
    pub const fn options(&self) -> &EndpointOptions {
        match self {
            Self::Tcp { options, .. } | Self::Tls { options, .. } | Self::Unix { options, .. } => {
                options
            }
        }
    }
}

impl FromStr for Endpoint {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((prefix, tail)) = s.split_once(':') else {
            return Err(Error::Endpoint(String::from(
                "invalid endpoint: missing prefix tcp: or tls: or unix:",
            )));
        };

        let (prefix, options) = if prefix.ends_with(']') {
            let Some((prefix, options)) = prefix.split_once('[') else {
                return Err(Error::Endpoint(String::from(
                    "missing '[' for endpoint options",
                )));
            };
            (
                prefix,
                EndpointOptions::from_str(&options[..options.len() - 1])?,
            )
        } else {
            (prefix, EndpointOptions::default())
        };

        match prefix {
            "tcp" => net::SocketAddr::from_str(tail)
                .map(|address| Self::Tcp { address, options })
                .map_err(|e| Error::Endpoint(format!("invalid socket addr for tcp endpoint: {e}"))),
            "tls" => net::SocketAddr::from_str(tail)
                .map(|address| Self::Tls { address, options })
                .map_err(|e| Error::Endpoint(format!("invalid socket addr for tls endpoint: {e}"))),
            "unix" => {
                let path = path::PathBuf::from(tail);
                Ok(Self::Unix { path, options })
            }
            _ => Err(Error::Endpoint(format!("unsupported prefix {prefix:?}"))),
        }
    }
}

struct EndpointVisitor;

impl serde::de::Visitor<'_> for EndpointVisitor {
    type Value = Endpoint;
    fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(fmt, "was expecting an endpoint definition")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Endpoint::from_str(v).map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Endpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(EndpointVisitor)
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "command-line", derive(clap::Parser))]
pub struct CommonConfig {
    #[serde(skip)]
    #[cfg(feature = "command-line")]
    #[cfg_attr(
        feature = "command-line",
        clap(
            value_name = "config_file_path",
            help = "Path to configuration file (will be read before applying command line arguments)"
        )
    )]
    pub(crate) config_file: Option<path::PathBuf>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            value_name = "1280..9000",
            help = "MTU of the link between sender and receiver"
        )
    )]
    mtu: Option<u16>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            value_name = "1..65535[, 1..65535]*",
            value_delimiter = ',',
            help = "Ports for UDP communications between sender and receiver",
        )
    )]
    ports: Option<Vec<u16>>,
    #[cfg_attr(
        feature = "command-line",
        clap(long, help = "Size in bytes of RaptorQ block")
    )]
    block: Option<u32>,
    #[cfg_attr(
        feature = "command-line",
        clap(long, help = "Number of repair RaptorQ packets")
    )]
    repair: Option<u16>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            value_name = "1..65535",
            help = "Maximal number of simultaneous clients connections"
        )
    )]
    pub max_clients: Option<u32>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            help = "Duration in seconds between sent/expected heartbeat message (0 to disable)"
        )
    )]
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
    pub fn heartbeat(&self) -> Option<time::Duration> {
        self.heartbeat
            .filter(|heartbeat| 0 < *heartbeat)
            .map(time::Duration::from_secs)
    }
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
#[cfg_attr(
    feature = "command-line",
    derive(clap::ValueEnum),
    clap(rename_all = "snake_case")
)]
pub enum TlsVersion {
    Tls1_1,
    Tls1_2,
    #[default]
    Tls1_3,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
#[cfg_attr(
    feature = "command-line",
    derive(clap::ValueEnum),
    clap(rename_all = "snake_case")
)]
#[allow(non_camel_case_types)]
pub enum TlsMethod {
    Mozilla_Intermediate_v4,
    Mozilla_Intermediate_v5,
    Mozilla_Modern_v4,
    #[default]
    Mozilla_Modern_v5,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "command-line", derive(clap::Parser))]
pub struct TlsConfig {
    #[cfg_attr(
        feature = "command-line",
        clap(
            value_name = "key_file_path",
            long = "tls-key",
            help = "Path to PEM key file"
        )
    )]
    pub(crate) key: Option<path::PathBuf>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            value_name = "certificate_file_path",
            long = "tls-certificate",
            help = "Path to PEM certificate file"
        )
    )]
    pub(crate) certificate: Option<path::PathBuf>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            value_name = "ca_file_path",
            long = "tls-ca",
            help = "Path to PEM accepted CA file"
        )
    )]
    pub(crate) ca: Option<path::PathBuf>,
    #[cfg_attr(
        feature = "command-line",
        clap(long = "tls-min", help = "Minimum TLS accepted version")
    )]
    pub(crate) tls_min: Option<TlsVersion>,
    #[cfg_attr(
        feature = "command-line",
        clap(long = "tls-method", help = "Minimum TLS accepted method")
    )]
    pub(crate) tls_method: Option<TlsMethod>,
    #[cfg_attr(
        feature = "command-line",
        clap(long = "tls-ciphers", help = "Accepted TLS ciphers")
    )]
    pub(crate) ciphers: Option<String>,
    #[cfg_attr(
        feature = "command-line",
        clap(long = "tls-groups", help = "Accepted TLS groups")
    )]
    pub(crate) groups: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "command-line", derive(clap::Parser))]
pub struct Receive {
    #[cfg_attr(feature = "command-line", clap(long, help = "Log level"))]
    log: Option<log::LevelFilter>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            value_name = "log4rs_config_file_path",
            help = "Path to log4rs config file"
        )
    )]
    log4rs_config: Option<path::PathBuf>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            value_name = "ip:port",
            help = "Listen socket address for Prometheus connections"
        )
    )]
    prometheus_listen: Option<net::SocketAddr>,
    #[cfg_attr(feature = "command-line", clap(long,
                                              value_parser = Endpoint::from_str,
                                              help = "Add a client endpoint [tcp:<ip:port>|tls:<ip:port>|unix:<socket_path>][,<flush,hash>=<true|false>]*"))]
    to: Vec<Endpoint>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            value_name = "ip",
            help = "IP address on which to listen from sender UDP packets"
        )
    )]
    from: Option<net::IpAddr>,
    #[cfg_attr(
        feature = "command-line",
        clap(long, help = "Mode used to receive UDP packets")
    )]
    mode: Option<Mode>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            help = "Maximum number of RaptorQ blocks to buffer for each client (0 means infinite)"
        )
    )]
    queue_size: Option<usize>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            help = "Duration in seconds without UDP packets before resetting the internal state of the RaptorQ receiver"
        )
    )]
    reset_timeout: Option<u64>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            help = "Duration in seconds without data for a client before closing the client connection"
        )
    )]
    abort_timeout: Option<u64>,
    #[cfg_attr(feature = "command-line", clap(flatten))]
    tls: Option<TlsConfig>,
}

impl Receive {
    #[must_use]
    pub(crate) fn log(&self) -> log::LevelFilter {
        self.log.unwrap_or(DEFAULT_LOG_LEVEL)
    }

    #[must_use]
    pub(crate) fn log4rs_config(&self) -> Option<path::PathBuf> {
        self.log4rs_config.clone()
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

    #[must_use]
    pub fn tls(&self) -> TlsConfig {
        self.tls.clone().unwrap_or_default()
    }

    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn tls_mut(&mut self) -> &mut TlsConfig {
        if self.tls.is_none() {
            self.tls = Some(TlsConfig::default());
        }
        self.tls.as_mut().unwrap()
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "command-line", derive(clap::Parser))]
pub struct Send {
    #[cfg_attr(feature = "command-line", clap(long, help = "Log level"))]
    log: Option<log::LevelFilter>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            value_name = "log4rs_config_file_path",
            help = "Path to log4rs config file"
        )
    )]
    log4rs_config: Option<path::PathBuf>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            value_name = "ip:port",
            help = "Listen socket address for Prometheus connections"
        )
    )]
    prometheus_listen: Option<net::SocketAddr>,
    #[cfg_attr(feature = "command-line", clap(long,
                                              value_parser = Endpoint::from_str,
                                              help = "Add a client endpoint [tcp:<ip:port>|tls:<ip:port>|unix:<socket_path>][,<flush,hash>=<true|false>]*"))]
    from: Vec<Endpoint>,
    #[cfg_attr(
        feature = "command-line",
        clap(long, value_name = "ip", help = "IP address of receiver")
    )]
    to: Option<net::IpAddr>,
    #[cfg_attr(
        feature = "command-line",
        clap(
            long,
            value_name = "ip:port",
            help = "Binding address of UDP socket used to reach reaceiver"
        )
    )]
    to_bind: Option<net::SocketAddr>,
    #[cfg_attr(
        feature = "command-line",
        clap(long, help = "Mode used to send UDP packets")
    )]
    mode: Option<Mode>,
    #[cfg_attr(feature = "command-line", clap(flatten))]
    tls: Option<TlsConfig>,
}

impl Send {
    #[must_use]
    pub(crate) fn log(&self) -> log::LevelFilter {
        self.log.unwrap_or(DEFAULT_LOG_LEVEL)
    }

    #[must_use]
    pub(crate) fn log4rs_config(&self) -> Option<path::PathBuf> {
        self.log4rs_config.clone()
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

    #[must_use]
    pub fn tls(&self) -> TlsConfig {
        self.tls.clone().unwrap_or_default()
    }

    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn tls_mut(&mut self) -> &mut TlsConfig {
        if self.tls.is_none() {
            self.tls = Some(TlsConfig::default());
        }
        self.tls.as_mut().unwrap()
    }
}

#[cfg_attr(feature = "command-line", derive(clap::Parser))]
#[derive(Default)]
pub struct ReceiveConfig {
    #[cfg_attr(feature = "command-line", clap(flatten))]
    pub common: CommonConfig,
    #[cfg_attr(feature = "command-line", clap(flatten))]
    pub receive: Receive,
}

impl From<Config> for ReceiveConfig {
    fn from(config: Config) -> Self {
        Self {
            common: config.common,
            receive: config.receive,
        }
    }
}

#[cfg_attr(feature = "command-line", derive(clap::Parser))]
#[derive(Default)]
pub struct SendConfig {
    #[cfg_attr(feature = "command-line", clap(flatten))]
    pub common: CommonConfig,
    #[cfg_attr(feature = "command-line", clap(flatten))]
    pub send: Send,
}

impl From<Config> for SendConfig {
    fn from(config: Config) -> Self {
        Self {
            common: config.common,
            send: config.send,
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(flatten)]
    pub(crate) common: CommonConfig,
    pub(crate) receive: Receive,
    pub(crate) send: Send,
}

pub(crate) fn parse(file: path::PathBuf) -> Result<Config, Error> {
    let mut file = fs::OpenOptions::new().read(true).write(false).open(file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(Config::deserialize(toml::Deserializer::parse(&content)?)?)
}
