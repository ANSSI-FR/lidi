use log::*;
use seccomp_sys::*;

// workaround until we can assume libseccomp >= 2.4.0 is always present
include!(concat!(env!("OUT_DIR"), "/const.rs"));

mod rule;
pub use rule::{Cmp, Comparator};

mod error;
pub use error::{Error, Result};

mod syscalls;
pub use syscalls::Syscall;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    KillProcess,
    KillThread,
    Trap,
    Errno(u16),
    Trace(u16),
    Allow,
}

impl Into<u32> for Action {
    fn into(self) -> u32 {
        use self::Action::*;
        match self {
            KillProcess => SCMP_ACT_KILL_PROCESS,
            KillThread => SCMP_ACT_KILL,
            Trap => SCMP_ACT_TRAP,
            Errno(e) => SCMP_ACT_ERRNO(e.into()),
            Trace(t) => SCMP_ACT_TRACE(t.into()),
            Allow => SCMP_ACT_ALLOW,
        }
    }
}

pub struct Context {
    ctx: *mut scmp_filter_ctx,
}

impl Context {
    pub fn init() -> Result<Context> {
        Context::init_with_action(DEFAULT_KILL)
    }

    pub fn init_with_action(default_action: Action) -> Result<Context> {
        let ctx = unsafe { seccomp_init(default_action.into()) };

        if ctx.is_null() {
            return Err(Error::from("seccomp_init returned null".to_string()));
        }

        Ok(Context { ctx })
    }

    #[inline]
    pub fn allow_syscall(&mut self, syscall: Syscall) -> Result<()> {
        self.set_action_for_syscall(Action::Allow, syscall)
    }

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

    pub fn load(&self) -> Result<()> {
        debug!("seccomp: loading policy");
        let ret = unsafe { seccomp_load(self.ctx) };

        if ret != 0 {
            Err(Error::from("seccomp_load returned error".to_string()))
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
}
