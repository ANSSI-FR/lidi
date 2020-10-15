// SPDX-License-Identifier: LGPL-3.0

#![feature(btree_drain_filter)]

use bincode::deserialize_from;
use log::info;
use std::{env, os::unix::net::UnixStream};
use syscallz::Syscall;

#[allow(unused)]
mod datagram;
#[allow(unused)]
mod errors;
#[allow(unused)]
mod security;
#[allow(unused)]
mod socket;
mod up;
#[allow(unused)]
mod utils;

use crate::{
    security::seccomp::setup_seccomp_profile,
    up::{config::WorkerConfig, worker::Worker},
};

pub fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        panic!("Usage: lidi-up-worker <process-name> <socket-path>");
    }

    crate::utils::setup_logger(args[1].clone());

    info!(
        "Connecting to the unix socket ({}) provided by the controller.",
        &args[2]
    );
    let mut controller_socket =
        UnixStream::connect(&args[2]).expect("Failed connecting to controller.");

    info!("Waiting for controller to send us the configuration.");
    let config: WorkerConfig =
        deserialize_from(&mut controller_socket).expect("Failed derserializing config.");

    info!("Worker {} starting with socket {}.", &args[1], &args[2]);
    let mut worker = Worker::new(controller_socket, &config).expect("Failed creating worker.");

    //
    // Setting up the epoll for the communication socket with the controller.
    //
    info!("setting up listener to communicate with the controller");
    worker
        .setup_listener()
        .expect("Failed setting up epoll for listener.");

    //
    // Set up the timer for file timeout
    //
    info!("setting up the timer to monitor for file timeout");
    worker
        .setup_expiration_timer()
        .expect("Failed setting up expiration timer.");

    //
    // Setting up seccomp policy.
    //
    // Allowed syscalls:
    //   - epoll_wait to wait for new transfers from the controller process
    //     through the pre-created socket pair.
    //   - rename is used to move files around the different directories.
    //   - recvmsg to receive new transfers from the controller process through the
    //     pre-created socket pair.
    //   - sendto in order to notify the controller of the end of a transfer.
    //   - write to write the received data to the file on disk.
    //   - close to be able to close the file handle at the end of the transfer.
    //   - fsetxattr to be able to write metadata to files extended attributes.
    //
    if !config.disable_seccomp {
        info!("seccomp activated; loading seccomp profile");
        setup_seccomp_profile(&[
            Syscall::epoll_wait,
            Syscall::rename,
            Syscall::recvmsg,
            Syscall::write,
            Syscall::sendto,
            Syscall::close,
            Syscall::fsetxattr,
            Syscall::brk,
            Syscall::mmap,
            Syscall::mremap,
            Syscall::munmap,
            Syscall::sigaltstack,
            Syscall::read,
        ]);
    }

    info!("Worker #{}: Starting main loop ...", &args[1]);
    worker.main_loop();
}
