// SPDX-License-Identifier: LGPL-3.0

use getrandom::getrandom;
use log::trace;
use nix::{
    mount::{mount, umount2, MntFlags, MsFlags},
    sched::{unshare, CloneFlags},
    unistd::{chdir, pivot_root},
};
use std::{
    fs::{create_dir, remove_dir},
    path::Path,
};

#[allow(dead_code)]
pub fn setup_root(path_to_new_root: &Path) {
    const NONE: Option<&'static [u8]> = None;

    let mut random_buffer = [0u8; 8];
    getrandom(&mut random_buffer).expect("Failed generating a name for the new root.");
    let random_name: String = random_buffer
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<String>>()
        .join("");

    let new_root_name = Path::new("/tmp").join(format!("lidi-root-{}", random_name));
    let old_root_path = new_root_name.join(&random_name);
    let old_root_name = Path::new("/").join(&random_name);

    trace!(
        "Mounting new root at {} and oldroot at {}.",
        new_root_name.display(),
        old_root_name.display()
    );

    // We start by putting ourselves in our own mount namespace.
    unshare(CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNS)
        .expect("Failed unsharing USER+MNT namespace.");

    // We start by remounting everything as private.
    // Note: This is because by default systemd mounts the root as MS_SHARED.
    mount(NONE, "/", NONE, MsFlags::MS_REC | MsFlags::MS_PRIVATE, NONE)
        .expect("Remounting recursively / as private failed.");

    // Creating the new root and bindmounting the fs on it
    create_dir(&new_root_name).expect("Failed creating new root directory.");
    mount(
        Some(path_to_new_root),
        &new_root_name,
        NONE,
        MsFlags::MS_BIND | MsFlags::MS_PRIVATE,
        NONE,
    )
    .expect("Bind mounting the new root failed.");

    // We pivot our root to the bind mounted rootfs so that the old root is
    // mounted in a sub-directory.
    create_dir(&old_root_path).expect("Failed creating old root directory.");
    pivot_root(&new_root_name, &old_root_path).expect("Pivoting the root from old to new failed.");

    // We change our directory to the root.
    chdir("/").expect("Changing working dir to / failed.");

    // We unmount the old root and all mount points below it.
    umount2(&old_root_name, MntFlags::MNT_DETACH).expect("Unmounting the old root failed.");

    // Finally, we remove the folder we used as a mount point for the old root.
    remove_dir(&old_root_name).expect("Failed to remove the old root folder.");
}
