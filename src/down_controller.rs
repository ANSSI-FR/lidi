// SPDX-License-Identifier: LGPL-3.0

use log::{error, info};
use std::fs::{read_dir, rename};
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
    down::{
        config::{Config, ExternalConfig},
        controller::Controller,
    },
    security::{namespaces::setup_root, seccomp::setup_seccomp_profile},
    utils::create_dirs_if_not_exists,
};

fn main() {
    crate::utils::setup_logger("controller".to_string());

    let config_filename = std::env::var("CONFIG_FILE_PATH")
        .unwrap_or("/etc/lidi/down.toml".to_owned());
    info!("Loading configuration from file {}.", config_filename);
    let config_file = std::fs::read_to_string(config_filename)
        .expect("Failed opening queues config file.");
    let external_config: ExternalConfig =
        toml::from_str(&config_file).expect("Failed parsing queues.");
    let config: Config = external_config.into();

    let mut controller = Controller::new(&config).expect("Failed setting up controller process.");

    //
    // Check the input directory where the transfers will be moved.
    //
    if !config.directory.is_dir() {
        panic!(
            "Input directory {} does not exist.",
            config.directory.display()
        );
    }

    // We "chroot" ourselves into that directory.
    if !config.disable_pivot_root {
        setup_root(&config.directory);
    }

    // Creates all the required directories: transfer, complete, failed and staging
    info!("Creating needed directories ...");
    for (_, queue) in config.queues.iter() {
        create_dirs_if_not_exists(&[
            &queue.path,
            &queue.worker_config.paths.staging,
            &queue.worker_config.paths.transfer,
            &queue.worker_config.paths.complete,
            &queue.worker_config.paths.failed,
        ]);
    }

    //
    // Setting up the stage before setting up inotify:
    // - everything that was in transfer is moved to failed
    // - everything in staging is listed to be put back in the queue
    //
    // /!\ THIS IS RACY: if anyone puts files in the staging directory before we
    // setup epoll and after we list files in staging, those files won't get
    // picked up.
    //
    let mut preexisting_staging = Vec::new();
    for (_, queue) in config.queues.iter() {
        info!("Reading all entries from transfer directory for leftover files ...");
        for entryopt in read_dir(&queue.worker_config.paths.transfer)
            .expect("Failed reading transfer directory for leftover files.")
        {
            if let Ok(entry) = entryopt {
                if entry.path().is_file() {
                    error!(
                        "Found file {} in transfer while booting; Discarding into failed.",
                        entry.file_name().to_string_lossy()
                    );
                    rename(
                        entry.path(),
                        queue.worker_config.paths.failed.join(entry.file_name()),
                    )
                    .expect("Failed moving file from transfer to failed.");
                }
            }
        }

        info!("Reading all entries from staging directory to add them in the queue ...");
        for entryopt in read_dir(&queue.worker_config.paths.staging)
            .expect("Failed reading staging directory for files.")
        {
            if let Ok(entry) = entryopt {
                if entry.path().is_file() {
                    let path = entry.path();
                    info!(
                        "Found file {} in staging; placed it in the preexisting list",
                        path.to_string_lossy()
                    );
                    preexisting_staging.push((queue.name.clone(), path))
                }
            }
        }
    }

    controller
        .setup_listener()
        .expect("Failed setting up unix socket listener.");

    controller.setup_inotify().expect("Failed setting inotify.");

    controller
        .setup_ratelimit_timer()
        .expect("Failed setting up rate-limit timer.");

    controller
        .spawn_workers()
        .expect("Failed spawning queue workers.");

    //
    // Sending the pre-existing files in staging to the workers.
    //
    info!("Sending preexisting files from staging directory ...");
    for (queue_name, filename) in preexisting_staging {
        if let Err(e) = controller.register_existing_file(queue_name, filename) {
            error!("{}", e)
        }
    }

    //
    // Tightening the seccomp policy before entering the main loop.
    //
    // Allowed syscalls:
    //   - epoll_wait to wait for new events from inotify.
    //   - open to open the new files being moved in the watched directory.
    //   - fgetxattr to get the extended attributes containing metadata to transfer.
    //   - sendmsg to send a message and the file descriptor previously opened to a worker.
    //   - read to read from inotify
    //   - write to be able to output logs.
    //   - nanosleep to sleep on the main loop.
    //
    if !config.disable_seccomp {
        setup_seccomp_profile(&[
            Syscall::epoll_wait,
            Syscall::openat,
            Syscall::lgetxattr,
            Syscall::sendmsg,
            Syscall::brk,
            Syscall::write,
            Syscall::read,
            Syscall::mmap,
            Syscall::mremap,
            Syscall::munmap,
            Syscall::close,
        ]);
    }

    info!("Starting main loop ...");
    controller.main_loop();
}
