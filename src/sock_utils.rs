use std::mem;
use std::os::fd::AsRawFd;

pub fn set_socket_send_buffer_size<S: AsRawFd>(socket: &S, size: usize) {
    unsafe {
        setsockopt_buffer_size(socket.as_raw_fd(), size as i32, libc::SO_SNDBUF);
    }
}

pub fn set_socket_recv_buffer_size<S: AsRawFd>(socket: &S, size: usize) {
    unsafe {
        setsockopt_buffer_size(socket.as_raw_fd(), size as i32, libc::SO_RCVBUF);
    }
}

unsafe fn setsockopt_buffer_size(fd: i32, size: i32, option_name: i32) {
    let res = libc::setsockopt(
        fd,
        libc::SOL_SOCKET,
        option_name,
        &size as *const libc::c_int as *const libc::c_void,
        mem::size_of::<libc::c_int>() as libc::socklen_t,
    );
    if res != 0 {
        log::error!("libc::setsockopt failed.");
        panic!();
    }
}
