use std::mem;
use std::os::fd::AsRawFd;

pub fn set_socket_send_buffer_size<S: AsRawFd>(socket: &S, size: i32) {
    unsafe {
        setsockopt_buffer_size(socket.as_raw_fd(), size, libc::SO_SNDBUF);
    }
}

pub fn set_socket_recv_buffer_size<S: AsRawFd>(socket: &S, size: i32) {
    unsafe {
        setsockopt_buffer_size(socket.as_raw_fd(), size, libc::SO_RCVBUF);
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

pub fn get_socket_send_buffer_size<S: AsRawFd>(socket: &S) -> i32 {
    unsafe { getsockopt_buffer_size(socket.as_raw_fd(), libc::SO_SNDBUF) }
}

pub fn get_socket_recv_buffer_size<S: AsRawFd>(socket: &S) -> i32 {
    unsafe { getsockopt_buffer_size(socket.as_raw_fd(), libc::SO_RCVBUF) }
}

unsafe fn getsockopt_buffer_size(fd: i32, option_name: i32) -> i32 {
    let mut sz = 0i32;
    let mut len = mem::size_of::<libc::c_int>() as libc::socklen_t;
    let res = libc::getsockopt(
        fd,
        libc::SOL_SOCKET,
        option_name,
        &mut sz as *mut libc::c_int as *mut libc::c_void,
        &mut len,
    );
    if res != 0 {
        log::error!("libc::getsockopt failed.");
        panic!();
    }
    sz
}
