// SPDX-License-Identifier: LGPL-3.0

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const STAGING_PATH: &str = "staging";
const TRANSFER_PATH: &str = "transfer";
const FAILED_PATH: &str = "failed";
const COMPLETE_PATH: &str = "complete";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Paths {
    pub staging: PathBuf,
    pub transfer: PathBuf,
    pub failed: PathBuf,
    pub complete: PathBuf,
}

impl Paths {
    pub fn new(base: &Path) -> Self {
        Self {
            staging: base.join(STAGING_PATH),
            transfer: base.join(TRANSFER_PATH),
            failed: base.join(FAILED_PATH),
            complete: base.join(COMPLETE_PATH),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ExternalQueue {
    pub weight: usize,
}

#[derive(Serialize, Deserialize)]
pub struct ExternalConfig {
    pub directory: PathBuf,
    pub socket: PathBuf,
    pub address: SocketAddr,

    pub bandwidth_limit: usize,
    pub max_in_flight: usize,
    pub max_packet_burst: usize,

    pub disable_seccomp: bool,
    pub disable_pivot_root: bool,
    pub disable_worker_ns: bool,

    pub queues: HashMap<String, ExternalQueue>,
}

impl Into<Config> for ExternalConfig {
    fn into(self) -> Config {
        let basedir = self.directory;
        let address = self.address;
        let max_packet_burst = self.max_packet_burst;
        let disable_seccomp = self.disable_seccomp;
        let disable_pivot_root = self.disable_pivot_root;
        let disable_worker_ns = self.disable_worker_ns;

        let queues: HashMap<String, Rc<QueueConfig>> = self
            .queues
            .into_iter()
            .map(|(k, q)| {
                (
                    k.to_owned(),
                    Rc::new(QueueConfig {
                        name: k.to_owned(),
                        path: basedir.join(&k),
                        weight: q.weight,

                        worker_config: WorkerConfig {
                            directory: basedir.clone(),
                            address,
                            paths: Paths::new(&basedir.join(&k)),

                            max_packet_burst,
                            disable_seccomp,
                            disable_pivot_root,
                            disable_worker_ns,
                        },
                    }),
                )
            })
            .collect();

        let total_nb_quantum = queues
            .values()
            .fold(0usize, |acc, q| acc + q.weight as usize);

        Config {
            directory: basedir,
            socket: self.socket,
            address: self.address,

            bandwidth_limit: self.bandwidth_limit,
            bandwidth_per_quantum: self.bandwidth_limit / total_nb_quantum,
            total_nb_quantum,

            max_in_flight: self.max_in_flight,
            max_packet_burst,

            disable_seccomp,
            disable_pivot_root,
            disable_worker_ns,

            queues,
        }
    }
}

#[derive(Clone)]
pub struct QueueConfig {
    pub name: String,

    pub path: PathBuf,
    pub weight: usize,

    pub worker_config: WorkerConfig,
}

pub struct Config {
    pub directory: PathBuf,
    pub socket: PathBuf,
    pub address: SocketAddr,

    pub bandwidth_limit: usize,
    pub bandwidth_per_quantum: usize,
    pub total_nb_quantum: usize,

    pub max_in_flight: usize,
    pub max_packet_burst: usize,

    pub disable_seccomp: bool,
    pub disable_pivot_root: bool,
    pub disable_worker_ns: bool,

    pub queues: HashMap<String, Rc<QueueConfig>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub directory: PathBuf,
    pub address: SocketAddr,
    pub paths: Paths,

    pub disable_seccomp: bool,
    pub disable_pivot_root: bool,
    pub disable_worker_ns: bool,

    pub max_packet_burst: usize,
}
