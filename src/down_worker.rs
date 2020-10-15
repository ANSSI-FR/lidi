// SPDX-License-Identifier: LGPL-3.0

use bincode::deserialize_from;
use log::info;
use std::env;
use std::os::unix::net::UnixStream;
use syscallz::Syscall;

#[allow(unused)]
mod datagram;
mod down;
#[allow(unused)]
mod errors;
#[allow(unused)]
mod security;
#[allow(unused)]
mod socket;
#[allow(unused)]
mod utils;

use crate::{
    down::{config::WorkerConfig, worker::Worker},
    security::seccomp::setup_seccomp_profile,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        panic!("Usage: lidi-down-worker <process-name> <socket-path>");
    }

    crate::utils::setup_logger(args[1].clone());

    info!(
        "Connecting to the unix socket ({}) provided by the controller.",
        &args[2]
    );
    let mut controller_socket =
        UnixStream::connect(&args[2]).expect("Failed connecting to controller.");

    info!("Waiting for controller to send us the memfd for synchronization.");

    info!("Waiting for controller to send us the configuration.");
    let config: WorkerConfig =
        deserialize_from(&mut controller_socket).expect("Failed deserializing config.");

    info!("waiting for memfd from controller");
    let atomic = Worker::wait_for_memfd_from_controller(&controller_socket)
        .expect("failed getting memfd from controller");

    info!("Worker {} starting with socket {}.", &args[1], &args[2]);
    let mut worker = Worker::new(args[1].clone(), controller_socket, atomic, &config)
        .expect("Failed creating worker.");

    info!("setting up listener on socket with controller");
    worker
        .setup_listener()
        .expect("Failed setting up epoll on socket with controller.");

    //
    // Setting up seccomp policy.
    //
    // Allowed syscalls:
    //   - epoll_wait to wait for new transfers from the controller process
    //     through the pre-created socket pair.
    //   - rename to move files between staging, transfer and complete directories
    //   - recvmsg to receive new transfers from the controller process through the
    //     pre-created socket pair.
    //   - sendto to send the packets over to the other side of the diode.
    //   - read to read from the file being sent.
    //   - write to be able to output logs.
    //   - close to be able to close the file handle at the end of the transfer.
    //   - getrandom for the UUID generation.
    //
    if !config.disable_seccomp {
        info!("seccomp activated; setting up seccomp filter");
        setup_seccomp_profile(&[
            Syscall::epoll_wait,
            Syscall::rename,
            Syscall::recvmsg,
            Syscall::sendto,
            Syscall::read,
            Syscall::write,
            Syscall::close,
            Syscall::getrandom,
            Syscall::brk,
            Syscall::mmap,
            Syscall::mremap,
            Syscall::munmap,
            Syscall::sigaltstack,
            Syscall::nanosleep,
        ]);
    }

    info!("Worker #{}: Starting main loop ...", &args[1]);
    worker.main_loop();
}
