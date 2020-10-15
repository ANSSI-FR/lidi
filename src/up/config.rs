// SPDX-License-Identifier: LGPL-3.0

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const TRANSFER_PATH: &str = "transfer";
const FAILED_PATH: &str = "failed";
const COMPLETE_PATH: &str = "complete";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Paths {
    pub transfer: PathBuf,
    pub failed: PathBuf,
    pub complete: PathBuf,
}

impl Paths {
    pub fn new(base: &Path) -> Self {
        Self {
            transfer: base.join(TRANSFER_PATH),
            failed: base.join(FAILED_PATH),
            complete: base.join(COMPLETE_PATH),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ExternalConfig {
    pub directory: PathBuf,
    pub address: SocketAddr,
    pub workers: NonZeroU64,

    pub disable_seccomp: bool,
    pub disable_pivot_root: bool,
    pub disable_worker_ns: bool,

    pub queues: Vec<String>,
}

impl Into<Config> for ExternalConfig {
    fn into(self) -> Config {
        let basedir = self.directory;
        let disable_seccomp = self.disable_seccomp;
        let disable_pivot_root = self.disable_pivot_root;
        let disable_worker_ns = self.disable_worker_ns;

        let queues: HashMap<String, Rc<QueueConfig>> = self
            .queues
            .into_iter()
            .map(|n| {
                (
                    n.to_owned(),
                    Rc::new(QueueConfig {
                        name: n.to_owned(),
                        path: basedir.join(&n),
                        dirs: Paths::new(&basedir.join(&n)),
                    }),
                )
            })
            .collect();

        let workers: Vec<Rc<WorkerConfig>> = (0..self.workers.get())
            .map(|_| {
                Rc::new(WorkerConfig {
                    disable_seccomp,
                    disable_pivot_root,
                    disable_worker_ns,

                    queues: queues
                        .iter()
                        .map(|(s, rc)| ((*s).clone(), (**rc).clone()))
                        .collect(),
                })
            })
            .collect();

        Config {
            directory: basedir,
            address: self.address,

            disable_seccomp,
            disable_pivot_root,
            disable_worker_ns,

            queues,
            workers,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QueueConfig {
    pub name: String,
    pub path: PathBuf,
    pub dirs: Paths,
}

pub struct Config {
    pub directory: PathBuf,
    pub address: SocketAddr,

    pub disable_seccomp: bool,
    pub disable_pivot_root: bool,
    pub disable_worker_ns: bool,

    pub queues: HashMap<String, Rc<QueueConfig>>,
    pub workers: Vec<Rc<WorkerConfig>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkerConfig {
    pub disable_seccomp: bool,
    pub disable_pivot_root: bool,
    pub disable_worker_ns: bool,

    pub queues: HashMap<String, QueueConfig>,
}
