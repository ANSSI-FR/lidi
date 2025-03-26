//! Bindings and wrappers for socket buffer size libc functions

use std::os::fd::AsRawFd;
use std::{io, mem, ptr};

pub fn set_socket_send_buffer_size<S: AsRawFd>(socket: &S, size: i32) -> Result<(), io::Error> {
    unsafe { setsockopt_buffer_size(socket.as_raw_fd(), size, libc::SO_SNDBUF) }
}

pub fn set_socket_recv_buffer_size<S: AsRawFd>(socket: &S, size: i32) -> Result<(), io::Error> {
    unsafe { setsockopt_buffer_size(socket.as_raw_fd(), size, libc::SO_RCVBUF) }
}

unsafe fn setsockopt_buffer_size(fd: i32, size: i32, option_name: i32) -> Result<(), io::Error> {
    let res = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            option_name,
            ptr::addr_of!(size).cast::<libc::c_void>(),
            mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };

    if res == 0 {
        Ok(())
    } else {
        Err(io::Error::other("libc::setsockopt"))
    }
}

pub fn get_socket_send_buffer_size<S: AsRawFd>(socket: &S) -> Result<i32, io::Error> {
    unsafe { getsockopt_buffer_size(socket.as_raw_fd(), libc::SO_SNDBUF) }
}

pub fn get_socket_recv_buffer_size<S: AsRawFd>(socket: &S) -> Result<i32, io::Error> {
    unsafe { getsockopt_buffer_size(socket.as_raw_fd(), libc::SO_RCVBUF) }
}

unsafe fn getsockopt_buffer_size(fd: i32, option_name: i32) -> Result<i32, io::Error> {
    let mut sz = 0i32;
    let mut len = mem::size_of::<libc::c_int>() as libc::socklen_t;
    let res = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            option_name,
            ptr::addr_of_mut!(sz).cast::<libc::c_void>(),
            &mut len,
        )
    };
    if res == 0 {
        Ok(sz)
    } else {
        Err(io::Error::other("libc::getsockopt"))
    }
}
