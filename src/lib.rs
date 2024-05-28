pub mod error;
pub mod file;
pub mod protocol;
pub mod receive;
pub mod send;

// Allow unsafe code to call libc function setsockopt.
#[allow(unsafe_code)]
pub mod sock_utils;

// Allow unsafe code to initialize C structs and call
// libc functions recv_mmsg and send_mmsg.
//#[allow(unsafe_code)]
//pub mod udp;

pub mod test;

use log::LevelFilter;
use log4rs::{
    append::console::{ConsoleAppender, Target},
    config::{Appender, Root},
    filter::threshold::ThresholdFilter,
    Config,
};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::{net::SocketAddr, str::FromStr};

pub fn init_logger(log_config: Option<&String>, debug: u8) {
    if let Some(file) = log_config {
        let _handle = log4rs::init_file(file, Default::default());
    } else {
        // Use this to change log levels at runtime.
        // This means you can change the default log level to trace
        // if you are trying to debug an issue and need more logs on
        let level = match debug {
            0 => LevelFilter::Info,
            1 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        };

        // Build a stderr logger.
        let stdout = ConsoleAppender::builder().target(Target::Stdout).build();
        // Log Trace level output to file where trace is the default level
        // and the programmatically specified level to stderr.
        let config = Config::builder()
            .appender(
                Appender::builder()
                    .filter(Box::new(ThresholdFilter::new(level)))
                    .build("stdout", Box::new(stdout)),
            )
            .build(Root::builder().appender("stdout").build(level))
            .unwrap();

        let _handle = log4rs::init_config(config).unwrap();
    }
}

pub fn init_metrics(prom_url: Option<&String>) {
    if let Some(addr) = prom_url {
        PrometheusBuilder::new()
            .with_http_listener(SocketAddr::from_str(addr).expect("Invalid metrics address"))
            .install()
            .unwrap();
    }
}
