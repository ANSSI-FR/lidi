//! Simple seccomp library for rust. Please note that the syscall list is
//! incomplete and you might need to send a PR to get your syscalls included. This
//! crate releases frequently if the syscall list has been updated.
//!
//! # Example
//!
//! ```no_run
//! use syscallz::{Context, Syscall, Action};
//!
//! fn main() -> syscallz::Result<()> {
//!
//!     // The default action if no other rule matches is syscallz::DEFAULT_KILL
//!     // For a different default use `Context::init_with_action`
//!     let mut ctx = Context::init()?;
//!
//!     // Allow-list some syscalls
//!     ctx.allow_syscall(Syscall::open);
//!     ctx.allow_syscall(Syscall::getpid);
//!     // Set a specific action for a syscall
//!     ctx.set_action_for_syscall(Action::Errno(1), Syscall::execve);
//!
//!     // Enforce the seccomp filter
//!     ctx.load()?;
//!
//!     Ok(())
//! }
//! ```

#![allow(bindings_with_variant_name)]

use log::*;
use seccomp_sys::*;
use std::os::unix::io::AsRawFd;

// workaround until we can assume libseccomp >= 2.4.0 is always present
include!(concat!(env!("OUT_DIR"), "/const.rs"));

mod rule;
pub use rule::{Cmp, Comparator};

mod error;
pub use error::{Error, Result};

mod syscalls;
pub use syscalls::Syscall;

/// The action to execute if a rule matches
#[derive(Debug, Clone, Copy)]
pub enum Action {
    /// Kill the whole process if a rule is violated.
    KillProcess,
    /// Kill the thread that has violated a rule.
    KillThread,
    /// Send a SIGSYS if a rule is violated.
    Trap,
    /// Reject the syscall and set the errno accordingly.
    Errno(u16),
    /// If the thread is being traced with ptrace, notify the tracing process.
    /// The numeric argument can be retrieved with `PTRACE_GETEVENTMSG`.
    Trace(u16),
    /// The syscall is allowed and executed as usual.
    Allow,
    // TODO: SCMP_ACT_TRAP
}

impl From<Action> for u32 {
    fn from(action: Action) -> u32 {
        use self::Action::*;
        match action {
            KillProcess => SCMP_ACT_KILL_PROCESS,
            KillThread => SCMP_ACT_KILL,
            Trap => SCMP_ACT_TRAP,
            Errno(e) => SCMP_ACT_ERRNO(e.into()),
            Trace(t) => SCMP_ACT_TRACE(t.into()),
            Allow => SCMP_ACT_ALLOW,
        }
    }
}

/// The context to configure and enforce seccomp rules
pub struct Context {
    ctx: *mut scmp_filter_ctx,
}

impl Context {
    /// Create a new seccomp context and use
    /// [`DEFAULT_KILL`](constant.DEFAULT_KILL.html) as the default action.
    pub fn init() -> Result<Context> {
        Context::init_with_action(DEFAULT_KILL)
    }

    /// Create a new seccomp context with the given Action as default action.
    pub fn init_with_action(default_action: Action) -> Result<Context> {
        let ctx = unsafe { seccomp_init(default_action.into()) };

        if ctx.is_null() {
            return Err(Error::from("seccomp_init returned null".to_string()));
        }

        Ok(Context { ctx })
    }

    /// Allow the given syscall regardless of the arguments.
    #[inline]
    pub fn allow_syscall(&mut self, syscall: Syscall) -> Result<()> {
        self.set_action_for_syscall(Action::Allow, syscall)
    }

    /// Execute the given action for the given syscall. This can be used to
    /// either allow or deny a syscall, regardless of the arguments.
    #[inline]
    pub fn set_action_for_syscall(&mut self, action: Action, syscall: Syscall) -> Result<()> {
        debug!("seccomp: setting action={:?} syscall={:?}", action, syscall);
        let ret = unsafe { seccomp_rule_add(self.ctx, action.into(), syscall.into_i32(), 0) };

        if ret != 0 {
            Err(Error::from("seccomp_rule_add returned error".to_string()))
        } else {
            Ok(())
        }
    }

    /// Execute a given action for a given syscall if the
    /// [`Comparator`](struct.Comparator.html)s match the given arguments.
    pub fn set_rule_for_syscall(
        &mut self,
        action: Action,
        syscall: Syscall,
        comparators: &[Comparator],
    ) -> Result<()> {
        debug!(
            "seccomp: setting action={:?} syscall={:?} comparators={:?}",
            action, syscall, comparators
        );
        let comps: Vec<scmp_arg_cmp> = comparators
            .iter()
            .map(|comp| comp.clone().into())
            .collect::<_>();

        let ret = unsafe {
            seccomp_rule_add_array(
                self.ctx,
                action.into(),
                syscall.into_i32(),
                comps.len() as u32,
                comps.as_ptr(),
            )
        };
        if ret != 0 {
            Err(Error::from(
                "seccomp_rule_add_array returned error".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Load and enforce the configured seccomp policy
    pub fn load(&self) -> Result<()> {
        debug!("seccomp: loading policy");
        let ret = unsafe { seccomp_load(self.ctx) };

        if ret != 0 {
            Err(Error::from("seccomp_load returned error".to_string()))
        } else {
            Ok(())
        }
    }

    /// Generate and output the current seccomp filter in BPF (Berkeley Packet
    /// Filter) format. The output is suitable to be loaded into the kernel. The
    /// filter is written to the given file descriptor.
    pub fn export_bpf(&self, fd: &mut dyn AsRawFd) -> Result<()> {
        let ret = unsafe { seccomp_export_bpf(self.ctx, fd.as_raw_fd()) };

        if ret != 0 {
            Err(Error::from("seccomp_export_bpf returned error".to_string()))
        } else {
            Ok(())
        }
    }

    /// Generate and output the current seccomp filter in PFC (Pseudo Filter
    /// Code) format. The output is human read and meant to be used for debugging
    /// for developers. The filter is written to the given file descriptor.
    pub fn export_pfc(&self, fd: &mut dyn AsRawFd) -> Result<()> {
        let ret = unsafe { seccomp_export_pfc(self.ctx, fd.as_raw_fd()) };

        if ret != 0 {
            Err(Error::from("seccomp_export_pfc returned error".to_string()))
        } else {
            Ok(())
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { seccomp_release(self.ctx) };
    }
}

#[cfg(test)]
mod tests {
    use super::syscalls::Syscall;
    use super::{Action, Context};
    use libc;

    // this test isn't fully stable yet
    #[test]
    #[ignore]
    fn it_works() {
        let mut ctx = Context::init_with_action(Action::Errno(69)).unwrap();
        ctx.allow_syscall(Syscall::futex).unwrap();
        ctx.load().unwrap();
        assert_eq!(unsafe { libc::getpid() }, -69);
    }

    #[test]
    fn from_name() {
        use crate::syscalls::Syscall;

        let cases = vec![
            ("write", Some(Syscall::write)),
            ("setgid", Some(Syscall::setgid)),
            ("nothing", None),
            ("", None),
        ];

        for (name, rhs) in cases {
            let lhs = Syscall::from_name(name);
            assert_eq!(lhs, rhs);
        }
    }

    #[test]
    fn test_rule() {
        use crate::rule::{Cmp, Comparator};
        use crate::Action;
        use std::fs::File;
        use std::io::Read;
        use std::os::unix::io::AsRawFd;

        let mut f = File::open("Cargo.toml").unwrap();

        let mut ctx = Context::init_with_action(Action::Allow).unwrap();
        ctx.set_rule_for_syscall(
            Action::Errno(1),
            Syscall::read,
            &[Comparator::new(0, Cmp::Eq, f.as_raw_fd() as u64, None)],
        )
        .unwrap();
        ctx.load().unwrap();

        let mut buf: [u8; 1024] = [0; 1024];
        let res = f.read(&mut buf);
        assert!(res.is_err());

        let err = res.unwrap_err();
        assert_eq!(err.raw_os_error(), Some(1));
    }

    #[test]
    fn test_export() {
        use std::fs::OpenOptions;

        let mut file = OpenOptions::new().append(true).open("/dev/null").unwrap();
        let ctx = Context::init_with_action(Action::Allow).unwrap();
        assert!(ctx.export_bpf(&mut file).is_ok());
        assert!(ctx.export_pfc(&mut file).is_ok());
    }
}
