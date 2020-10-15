// SPDX-License-Identifier: LGPL-3.0
use nix::{errno::Errno, sys::stat::Mode, unistd::mkdir};
use std::io::Write;
use std::path::Path;

pub fn create_dirs_if_not_exists(dirs: &[&Path]) {
    for dir in dirs.iter() {
        match mkdir(*dir, Mode::S_IRWXU) {
            Ok(_) => {}
            Err(e) => match e.as_errno() {
                Some(Errno::EEXIST) => {}
                _ => panic!(
                    "Creation of directory {} failed with error: {}",
                    dir.display(),
                    e
                ),
            },
        }
    }
}

pub fn setup_logger(program_name: String) {
    let mut builder = env_logger::Builder::from_default_env();
    builder.format(move |buf, record| {
        writeln!(
            buf,
            "[{}] {}: {}",
            record.level(),
            program_name,
            record.args()
        )
    });
    builder.init();
}
