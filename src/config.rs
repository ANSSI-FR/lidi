use core_affinity::CoreId;
use serde::Deserialize;
use std::io::prelude::*;
use std::io::{Error, ErrorKind, Result};

#[derive(Deserialize)]
pub struct DiodeConfig {
    /// Size of RaptorQ block, in bytes
    pub encoding_block_size: u64,
    /// Size of repair data, in bytes
    pub repair_block_size: u32,
    /// IP address on diode-receive side used to transfert UDP packets between diode-send and diode-receive
    pub udp_addr: String,
    /// List of ports used to transfert packets between diode-send and diode-receive. Each different port will create a new thread. Each port/thread is able to process up to 3 Gb/s.
    pub udp_port: Vec<u16>,
    /// MTU of the to use one the UDP link
    pub udp_mtu: u16,
    /// heartbeat period in ms
    pub heartbeat: u32,
    /// Path to log configuration file
    pub log_config: Option<String>,
    /// diode sender options
    pub sender: Option<DiodeSenderConfig>,
    /// diode receiver options
    pub receiver: Option<DiodeReceiverConfig>,
}

#[derive(Deserialize)]
pub struct DiodeSenderConfig {
    /// TCP server socket to accept data
    pub bind_tcp: String,
    /// UDP socket src address to send data (format A.B.C.D or A.B.C.D:P)
    pub bind_udp: String,
    /// ratelimit TCP session speed (in Mbit/s)
    pub max_bandwidth: Option<f64>,
    /// prometheus port (sender)
    pub metrics: Option<String>,
}

#[derive(Deserialize)]
pub struct DiodeReceiverConfig {
    /// IP address and port of the TCP server
    pub to_tcp: String,
    /// Timeout before force incomplete block recovery (in ms). Default is equal to heartbeat interval.
    pub block_expiration_timeout: Option<u32>,
    /// Session expiration delay. Time to wait before changing session (in s). Default is equal to 2 x heartbeat interval.
    pub session_expiration_timeout: Option<u32>,
    /// List of core affinity. One different core per thread. Each core id must exists.
    pub core_affinity: Option<Vec<usize>>,
    /// prometheus port (receiver)
    pub metrics: Option<String>,
    /// Size of the queue between UDP receiver and block reorder/decoder. Default is 10k packets.
    pub udp_packets_queue_size: Option<usize>,
    /// Size of the queue between block reorder/decoder and TCP sender. Default is 1k blocks.
    pub tcp_blocks_queue_size: Option<usize>,
}

pub const MAX_MTU: usize = 9000;

impl DiodeConfig {
    pub fn load(path: &str) -> Result<DiodeConfig> {
        let mut file = std::fs::File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let config: DiodeConfig = toml::from_str(&contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e}")))?;

        DiodeConfig::check_mtu(config.udp_mtu)?;
        DiodeConfig::check_threads_and_ports(&config)?;
        DiodeConfig::check_threads_and_core_affinity(&config)?;

        Ok(config)
    }

    fn check_mtu(mtu: u16) -> Result<()> {
        if mtu > MAX_MTU as _ {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid MTU: {mtu}: must be < 9000"),
            ));
        }

        Ok(())
    }

    fn check_threads_and_ports(config: &DiodeConfig) -> Result<()> {
        if config.udp_port.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid 'udp_port' list: port list is empty".to_string(),
            ));
        }

        let mut dedup_list = config.udp_port.clone();
        dedup_list.dedup();

        if config.udp_port.len() != dedup_list.len() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Invalid 'udp_port' list: there are duplicated values: {:?}",
                    config.udp_port
                ),
            ));
        }

        Ok(())
    }

    fn check_threads_and_core_affinity(config: &DiodeConfig) -> Result<()> {
        if let Some(receiver) = &config.receiver {
            if let Some(core_affinity) = &receiver.core_affinity {
                let mut dedup_list = core_affinity.clone();
                dedup_list.dedup();

                if core_affinity.len() != dedup_list.len() {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "Invalid 'receiver.core_affinity' list: there are duplicated values: {:?}",
                            core_affinity
                        ),
                    ));
                }

                // check there are only usable cores
                match core_affinity::get_core_ids() {
                    Some(core_ids) => {
                        for core in core_affinity {
                            if !core_ids.contains(&CoreId { id: *core }) {
                                return Err(Error::new(
                                    ErrorKind::InvalidData,
                                    format!(
                                        "Invalid 'receiver.core_affinity' list: impossible to run on core {}",
                                        core
                                    ),
                                ));
                            }
                        }
                    }
                    None => {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            "Unable to get core list".to_string(),
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}
