use strum_macros::EnumString;

#[cfg(target_arch = "arm")]
include!("b32/arm.rs");

#[cfg(target_arch = "powerpc")]
include!("b32/powerpc.rs");

// include!("b32/sparc.rs");

#[cfg(target_arch="x86")]
include!("b32/x86.rs");

#[cfg(target_arch="mips")]
include!("b32/mips.rs");

#[cfg(target_arch = "powerpc64")]
include!("b64/powerpc64.rs");

#[cfg(target_arch = "s390x")]
include!("b64/s390x.rs");

#[cfg(target_arch = "sparc64")]
include!("b64/sparc64.rs");

#[cfg(target_arch="x86_64")]
include!("b64/x86_64.rs");

#[cfg(target_arch="riscv64")]
include!("b64/riscv64.rs");

#[cfg(target_arch="aarch64")]
include!("b64/aarch64.rs");

#[cfg(target_arch="mips64")]
include!("b64/mips64.rs");

impl Syscall {
    #[inline]
    pub fn into_i32(self) -> i32 {
        self as i32
    }

    #[inline]
    pub fn from_name(name: &str) -> Option<Self>{
        use std::str::FromStr;
        Self::from_str(name).ok()
    }
}
