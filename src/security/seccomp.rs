// SPDX-License-Identifier: LGPL-3.0

use syscallz::{Action, Context, Syscall};

#[allow(dead_code)]
pub fn setup_seccomp_profile(syscalls: &[Syscall]) {
    let mut seccomp =
        Context::init_with_action(Action::KillProcess).expect("Failed setting up seccomp context.");

    for syscall in syscalls {
        seccomp
            .allow_syscall(*syscall)
            .expect("Failed allowing syscall with seccomp.");
    }

    seccomp.load().expect("Failed loading seccomp context.");
}
