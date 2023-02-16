pub mod file;
pub mod protocol;
pub mod receive;
pub mod semaphore;
pub mod send;

// Allow unsafe code to call libc function setsockopt.
#[allow(unsafe_code)]
pub(crate) mod sock_utils;

// Allow unsafe code to initialize C structs and call
// libc functions recv_mmsg and send_mmsg.
#[allow(unsafe_code)]
pub mod udp;
