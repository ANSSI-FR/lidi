use std::{io, mem, net, pin, ptr};

pub const MAX_MMSG_BATCH_SIZE: u32 = 1024;

pub fn convert_address(
    dest: net::SocketAddr,
) -> Result<(pin::Pin<Box<libc::sockaddr>>, usize), io::Error> {
    let (dest, dest_len) = match dest {
        net::SocketAddr::V4(addr4) => {
            let addr = libc::sockaddr_in {
                sin_family: libc::sa_family_t::try_from(libc::AF_INET).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("libc::AF_INET: {e}"))
                })?,
                sin_addr: libc::in_addr {
                    s_addr: u32::from_le_bytes(addr4.ip().octets()),
                },
                sin_port: addr4.port().to_be(),
                sin_zero: [0; 8],
            };
            let addr = Box::new(addr);
            (
                unsafe {
                    mem::transmute::<
                        std::boxed::Box<libc::sockaddr_in>,
                        std::boxed::Box<libc::sockaddr>,
                    >(addr)
                },
                mem::size_of::<libc::sockaddr_in>(),
            )
        }
        net::SocketAddr::V6(addr6) => {
            let addr = libc::sockaddr_in6 {
                sin6_family: libc::sa_family_t::try_from(libc::AF_INET6).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("libc::AF_INET6: {e}"))
                })?,
                sin6_port: addr6.port().to_be(),
                sin6_flowinfo: addr6.flowinfo(),
                sin6_addr: libc::in6_addr {
                    s6_addr: addr6.ip().octets(),
                },
                sin6_scope_id: addr6.scope_id(),
            };
            let addr = Box::new(addr);
            (
                unsafe {
                    mem::transmute::<
                        std::boxed::Box<libc::sockaddr_in6>,
                        std::boxed::Box<libc::sockaddr>,
                    >(addr)
                },
                mem::size_of::<libc::sockaddr_in6>(),
            )
        }
    };

    Ok((pin::Pin::new(dest), dest_len))
}

pub fn getsockopt_buffer_size(fd: i32, option_name: i32) -> Result<i32, io::Error> {
    let mut sz = 0i32;
    let mut len = libc::socklen_t::try_from(mem::size_of::<libc::c_int>())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("len: {e}")))?;
    let res = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            option_name,
            ptr::addr_of_mut!(sz).cast::<libc::c_void>(),
            &raw mut len,
        )
    };
    if res == 0 {
        Ok(sz)
    } else {
        Err(io::Error::other("libc::getsockopt"))
    }
}

pub fn setsockopt_buffer_size(fd: i32, size: i32, option_name: i32) -> Result<(), io::Error> {
    let len = libc::socklen_t::try_from(mem::size_of::<libc::c_int>())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("len: {e}")))?;

    let res = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            option_name,
            ptr::addr_of!(size).cast::<libc::c_void>(),
            len,
        )
    };

    if res == 0 {
        Ok(())
    } else {
        Err(io::Error::other("libc::setsockopt"))
    }
}
