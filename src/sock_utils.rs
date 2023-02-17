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
    let current_size = getsockopt_buffer_size(fd, option_name);
    if current_size >= size {
        return;
    }

    log::warn!(
        "default socket buffer size may be too small ({current_size} < {}), adjusting it",
        size
    );
    let size = size / 2;

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

    let new_size = getsockopt_buffer_size(fd, option_name);
    log::info!("socket buffer size set to {new_size}");
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
