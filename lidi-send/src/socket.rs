use lidi_command_utils::{config, socket};
#[cfg(feature = "send-mmsg")]
use std::num;
use std::{io, net, os::fd::AsRawFd};
#[cfg(any(feature = "send-msg", feature = "send-mmsg"))]
use std::{mem, pin};

pub fn get_socket_send_buffer_size<S: AsRawFd>(socket: &S) -> Result<i32, io::Error> {
    socket::getsockopt_buffer_size(socket.as_raw_fd(), libc::SO_SNDBUF)
}

pub fn set_socket_send_buffer_size<S: AsRawFd>(socket: &S, size: i32) -> Result<(), io::Error> {
    socket::setsockopt_buffer_size(socket.as_raw_fd(), size, libc::SO_SNDBUF)
}

pub enum Send {
    #[cfg(feature = "send-native")]
    Native {
        socket: net::UdpSocket,
        dest: net::SocketAddr,
    },
    #[cfg(feature = "send-msg")]
    Msg {
        socket: net::UdpSocket,
        _dest: pin::Pin<Box<libc::sockaddr>>,
        msghdr: libc::msghdr,
        iovec: pin::Pin<Box<libc::iovec>>,
    },
    #[cfg(feature = "send-mmsg")]
    Mmsg {
        socket: net::UdpSocket,
        _dest: pin::Pin<Box<libc::sockaddr>>,
        mmsghdr: Vec<libc::mmsghdr>,
        iovecs: pin::Pin<Vec<libc::iovec>>,
    },
}

impl Send {
    pub fn new(
        socket: net::UdpSocket,
        dest: net::SocketAddr,
        mode: config::Mode,
    ) -> Result<Self, io::Error> {
        match mode {
            config::Mode::Native => {
                #[cfg(not(feature = "send-native"))]
                {
                    Err(io::Error::new(io::ErrorKind::Unsupported, "send-native"))
                }
                #[cfg(feature = "send-native")]
                {
                    Ok(Self::Native { socket, dest })
                }
            }
            config::Mode::Msg => {
                #[cfg(not(feature = "send-msg"))]
                {
                    Err(io::Error::new(io::ErrorKind::Unsupported, "send-msg"))
                }
                #[cfg(feature = "send-msg")]
                {
                    let (mut dest, dest_len) = socket::convert_address(dest)?;
                    let raw_dest = (&raw mut *dest).cast::<libc::sockaddr>();
                    let dest_len = u32::try_from(dest_len).map_err(|e| {
                        io::Error::new(io::ErrorKind::InvalidData, format!("dest_len: {e}"))
                    })?;

                    let iovec = unsafe { mem::zeroed::<libc::iovec>() };
                    let mut iovec = pin::Pin::new(Box::new(iovec));

                    let mut msghdr = unsafe { mem::zeroed::<libc::msghdr>() };

                    msghdr.msg_name = raw_dest.cast::<libc::c_void>();
                    msghdr.msg_namelen = dest_len;
                    msghdr.msg_iov = &raw mut *iovec;
                    msghdr.msg_iovlen = 1;

                    Ok(Self::Msg {
                        socket,
                        _dest: dest,
                        msghdr,
                        iovec,
                    })
                }
            }
            config::Mode::Mmsg => {
                #[cfg(not(feature = "send-mmsg"))]
                {
                    Err(io::Error::new(io::ErrorKind::Unsupported, "send-native"))
                }
                #[cfg(feature = "send-mmsg")]
                {
                    let (mut dest, dest_len) = socket::convert_address(dest)?;
                    let raw_dest = (&raw mut *dest).cast::<libc::sockaddr>();
                    let dest_len = u32::try_from(dest_len).map_err(|e| {
                        io::Error::new(io::ErrorKind::InvalidData, format!("dest_len: {e}"))
                    })?;

                    let iovecs = vec![
                        unsafe { mem::zeroed::<libc::iovec>() };
                        socket::MAX_MMSG_BATCH_SIZE as usize
                    ];
                    let mut iovecs = pin::Pin::new(iovecs);

                    let mut mmsghdr = vec![
                        unsafe { mem::zeroed::<libc::mmsghdr>() };
                        socket::MAX_MMSG_BATCH_SIZE as usize
                    ];

                    for i in 0..socket::MAX_MMSG_BATCH_SIZE as usize {
                        mmsghdr[i].msg_hdr.msg_name = raw_dest.cast::<libc::c_void>();
                        mmsghdr[i].msg_hdr.msg_namelen = dest_len;
                        mmsghdr[i].msg_hdr.msg_iov = &raw mut iovecs[i];
                        mmsghdr[i].msg_hdr.msg_iovlen = 1;
                    }

                    Ok(Self::Mmsg {
                        socket,
                        _dest: dest,
                        mmsghdr,
                        iovecs,
                    })
                }
            }
        }
    }

    pub fn send(&mut self, packets: &[raptorq::EncodingPacket]) -> Result<(), io::Error> {
        let datagrams = packets.iter().map(raptorq::EncodingPacket::serialize);

        match self {
            #[cfg(feature = "send-native")]
            Self::Native { socket, dest } => {
                for datagram in datagrams {
                    let len = datagram.len();

                    let sent = socket.send_to(&datagram, *dest)?;

                    if sent != len {
                        return Err(io::Error::other(format!(
                            "libc::sendmsg failed {sent} != {len}"
                        )));
                    }
                }
            }
            #[cfg(feature = "send-msg")]
            Self::Msg {
                socket,
                msghdr,
                iovec,
                ..
            } => {
                for mut datagram in datagrams {
                    let len = datagram.len();

                    iovec.iov_base = datagram.as_mut_ptr().cast();
                    iovec.iov_len = len;

                    let sent = unsafe { libc::sendmsg(socket.as_raw_fd(), msghdr, 0) };

                    if sent != len.cast_signed() {
                        return Err(io::Error::other(format!(
                            "libc::sendmsg failed {sent} != {len}"
                        )));
                    }
                }
            }
            #[cfg(feature = "send-mmsg")]
            Self::Mmsg {
                socket,
                mmsghdr,
                iovecs,
                ..
            } => {
                for datagrams in datagrams
                    .collect::<Vec<_>>()
                    .chunks_mut(socket::MAX_MMSG_BATCH_SIZE as usize)
                {
                    let to_send = datagrams.len();

                    for (i, datagram) in datagrams.iter_mut().enumerate() {
                        mmsghdr[i].msg_len =
                            u32::try_from(datagram.len()).map_err(|e: num::TryFromIntError| {
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("datagram.len(): {e}"),
                                )
                            })?;
                        iovecs[i].iov_base = datagram.as_mut_ptr().cast::<libc::c_void>();
                        iovecs[i].iov_len = datagram.len();
                    }

                    let sent = unsafe {
                        libc::sendmmsg(
                            socket.as_raw_fd(),
                            mmsghdr.as_mut_ptr(),
                            u32::try_from(to_send).map_err(|e| {
                                io::Error::new(io::ErrorKind::InvalidData, format!("to_send: {e}"))
                            })?,
                            0,
                        ) as isize
                    };

                    if sent.cast_unsigned() != to_send {
                        return Err(io::Error::other("libc::sendmmsg"));
                    }
                }
            }
        }

        Ok(())
    }
}
