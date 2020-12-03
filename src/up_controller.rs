// SPDX-License-Identifier: LGPL-3.0

#![feature(btree_drain_filter)]

use log::info;
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
    security::{namespaces::setup_root, seccomp::setup_seccomp_profile},
    up::{
        config::{Config, ExternalConfig},
        controller::Controller,
    },
    utils::create_dirs_if_not_exists,
};

fn main() {
    crate::utils::setup_logger("controller".to_string());

    let config_filename =
        std::env::var("CONFIG_FILE_PATH").unwrap_or("/etc/lidi/up.toml".to_owned());
    info!("Loading configuration from file {}.", config_filename);
    let config_file =
        std::fs::read_to_string(config_filename).expect("Failed opening queues config file.");
    let external_config: ExternalConfig =
        toml::from_str(&config_file).expect("Failed parsing queues.");

    let config: Config = external_config.into();

    let mut controller = Controller::new(&config).expect("Failed building new controller.");

    //
    // Check the output directory where the transfers will be copied.
    //
    if !config.directory.is_dir() {
        panic!(
            "Output directory {} does not exist.",
            config.directory.display()
        );
    }

    // We "chroot" ourselves into that directory.
    if !config.disable_pivot_root {
        info!("Pivot root is activated; pivoting.");
        setup_root(&config.directory);
    }

    // Creates all the required directories: transfer, complete, failed
    info!("Creating needed directories ...");
    for (_, queue) in config.queues.iter() {
        create_dirs_if_not_exists(&[
            &queue.path,
            &queue.dirs.transfer,
            &queue.dirs.complete,
            &queue.dirs.failed,
        ]);
    }

    info!("Setting up epoll for the UDP socket ...");
    controller
        .setup_socket()
        .expect("Failed setting up epoll on main socket.");

    //
    // Spawning workers
    //
    info!("Spawning workers ...");
    controller
        .spawn_workers()
        .expect("failed spawning workers.");

    //
    // Tightening the seccomp policy before entering the main loop.
    //
    // Allowed syscalls:
    //   - epoll_wait to wait for new events from inotify.
    //   - recvfrom to receive new datagrams from the socket.
    //   - sendto to send message forward to workers.
    //   - openat to open the new files being transfered.
    //   - sendmsg to send the initial file descriptor to workers.
    //   - write to be able to output logs.
    //   - nanosleep to sleep on the main loop.
    //   - close to close files after they have been processed.
    //
    if !config.disable_seccomp {
        info!("Seccomp is activated; applying seccomp filter.");
        setup_seccomp_profile(&[
            Syscall::epoll_wait,
            Syscall::openat,
            Syscall::recvfrom,
            Syscall::sendto,
            Syscall::close,
            Syscall::sendmsg,
            Syscall::brk,
            Syscall::write,
            Syscall::nanosleep,
            Syscall::mmap,
            Syscall::mremap,
            Syscall::munmap,
            Syscall::futex,
        ]);
    }

    info!("Starting main loop ...");
    controller.main_loop();
}
