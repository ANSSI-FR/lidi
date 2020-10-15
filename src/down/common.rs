// SPDX-License-Identifier: LGPL-3.0

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{io::Write, os::unix::net::UnixStream};

use crate::errors::Result;

#[derive(Deserialize)]
pub struct SendFileCommand {
    pub queue: String,
    pub metadata: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
pub struct WorkerMessage {
    pub filename: Option<PathBuf>,
    pub metadata: Option<Vec<u8>>,
}

#[allow(dead_code)]
pub fn notify_controller(socket: &mut UnixStream) -> Result<()> {
    let buffer = [0u8; 8];
    socket.write_all(&buffer).map_err(From::from)
}
