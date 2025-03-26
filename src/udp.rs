//! Functions and wrappers over libc's UDP socket multiple messages receive and send

use std::marker::PhantomData;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};
use std::{io, mem, net, thread};

pub struct UdpRecv;
pub struct UdpSend;

/// Wrapper structure over the socket and buffers used to send and receive multiple messages.
/// Inner data are used to call libc recvmmsg and sendmmsg.
///
/// The `D` type parameter is intended to be [UdpRecv] or [UdpSend] to ensure structures are
/// correctly initialized according to the data transfer direction.
pub struct UdpMessages<D> {
    socket: net::UdpSocket,
    vlen: usize,
    _sockaddr: Option<Box<libc::sockaddr>>,
    msgvec: Vec<libc::mmsghdr>,
    iovecs: Vec<libc::iovec>,
    buffers: Vec<Vec<u8>>,
    marker: PhantomData<D>,
    bandwidth_limit: f64,
}

impl<D> UdpMessages<D> {
    fn new(
        socket: net::UdpSocket,
        vlen: usize,
        msglen: Option<usize>,
        addr: Option<net::SocketAddr>,
        bandwidth_limit: f64,
    ) -> Self {
        let (mut msgvec, mut iovecs, mut buffers);

        unsafe {
            msgvec = vec![mem::zeroed::<libc::mmsghdr>(); vlen];
            iovecs = vec![mem::zeroed::<libc::iovec>(); vlen];
            if let Some(msglen) = msglen {
                buffers = vec![vec![mem::zeroed::<u8>(); msglen]; vlen];
            } else {
                buffers = Vec::new();
            }
        }

        let mut sockaddr: Option<Box<libc::sockaddr>> = addr.map(|addr| match addr {
            net::SocketAddr::V4(addr4) => {
                let sockaddr_in = Box::new(libc::sockaddr_in {
                    sin_family: libc::AF_INET as libc::sa_family_t,
                    sin_addr: libc::in_addr {
                        s_addr: u32::from_le_bytes(addr4.ip().octets()),
                    },
                    sin_port: addr4.port().to_be(),
                    ..unsafe { mem::zeroed() }
                });
                unsafe {
                    mem::transmute::<
                        std::boxed::Box<libc::sockaddr_in>,
                        std::boxed::Box<libc::sockaddr>,
                    >(sockaddr_in)
                }
            }
            net::SocketAddr::V6(addr6) => {
                let sockaddr_in6 = Box::new(libc::sockaddr_in6 {
                    sin6_family: libc::AF_INET6 as libc::sa_family_t,
                    sin6_port: addr6.port().to_be(),
                    sin6_flowinfo: addr6.flowinfo(),
                    sin6_addr: libc::in6_addr {
                        s6_addr: addr6.ip().octets(),
                    },
                    sin6_scope_id: addr6.scope_id(),
                });
                unsafe {
                    mem::transmute::<
                        std::boxed::Box<libc::sockaddr_in6>,
                        std::boxed::Box<libc::sockaddr>,
                    >(sockaddr_in6)
                }
            }
        });

        for i in 0..vlen {
            if let Some(msglen) = msglen {
                iovecs[i].iov_base = buffers[i].as_mut_ptr().cast::<libc::c_void>();
                iovecs[i].iov_len = msglen;
            }
            if let Some(sockaddr) = &mut sockaddr {
                msgvec[i].msg_hdr.msg_name =
                    (sockaddr.as_mut() as *mut libc::sockaddr).cast::<libc::c_void>();
                msgvec[i].msg_hdr.msg_namelen = mem::size_of::<libc::sockaddr_in>() as u32;
            }
            msgvec[i].msg_hdr.msg_iov = &mut iovecs[i];
            msgvec[i].msg_hdr.msg_iovlen = 1;
        }

        Self {
            socket,
            vlen,
            _sockaddr: sockaddr,
            msgvec,
            iovecs,
            buffers,
            marker: PhantomData,
            bandwidth_limit,
        }
    }
}

impl UdpMessages<UdpRecv> {
    pub fn new_receiver(socket: net::UdpSocket, vlen: usize, msglen: usize) -> Self {
        log::info!("UDP configured to receive {vlen} messages (datagrams)");
        Self::new(socket, vlen, Some(msglen), None, 0.0)
    }

    pub fn recv_mmsg(&mut self) -> Result<impl Iterator<Item = &[u8]>, io::Error> {
        let nb_msg = unsafe {
            libc::recvmmsg(
                self.socket.as_raw_fd(),
                self.msgvec.as_mut_ptr(),
                self.vlen as u32,
                libc::MSG_WAITFORONE,
                std::ptr::null_mut(),
            )
        };

        if nb_msg == -1 {
            Err(io::Error::other("libc::recvmmsg"))
        } else {
            Ok(self
                .buffers
                .iter()
                .take(nb_msg as usize)
                .zip(self.msgvec.iter())
                .map(|(buffer, msghdr)| &buffer[..msghdr.msg_len as usize]))
        }
    }
}

impl UdpMessages<UdpSend> {
    pub fn new_sender(
        socket: net::UdpSocket,
        vlen: usize,
        dest: net::SocketAddr,
        bandwidth_limit: f64,
    ) -> UdpMessages<UdpSend> {
        log::info!("UDP configured to send {vlen} messages (datagrams) at a time");
        Self::new(socket, vlen, None, Some(dest), bandwidth_limit)
    }

    pub fn send_mmsg(&mut self, mut buffers: Vec<Vec<u8>>) -> Result<(), io::Error> {
        for bufchunk in buffers.chunks_mut(self.vlen) {
            if self.bandwidth_limit > 0.0 {
                for (i, buf) in bufchunk.iter_mut().enumerate() {
                    self.msgvec[i].msg_len = buf.len() as u32;
                    self.iovecs[i].iov_base = buf.as_mut_ptr().cast::<libc::c_void>();
                    self.iovecs[i].iov_len = buf.len();

                    let start_time = Instant::now();
                    let nb_msg;
                    unsafe {
                        nb_msg = libc::sendmmsg(self.socket.as_raw_fd(), &mut self.msgvec[i], 1, 0);
                    }

                    if nb_msg == -1 {
                        return Err(io::Error::other("libc::sendmmsg"));
                    }

                    let send_duration = start_time.elapsed().as_secs_f64();
                    let bytes_sent = buf.len() as f64;
                    let ideal_time_per_byte = 1.0 / self.bandwidth_limit;
                    let ideal_send_duration = bytes_sent * ideal_time_per_byte;
                    let sleep_duration = if ideal_send_duration > send_duration {
                        Duration::from_secs_f64(ideal_send_duration - send_duration)
                    } else {
                        Duration::from_secs(0)
                    };

                    thread::sleep(sleep_duration);
                }
            } else {
                let to_send = bufchunk.len();

                for (i, buf) in bufchunk.iter_mut().enumerate() {
                    self.msgvec[i].msg_len = buf.len() as u32;
                    self.iovecs[i].iov_base = buf.as_mut_ptr().cast::<libc::c_void>();
                    self.iovecs[i].iov_len = buf.len();
                }

                let nb_msg;
                unsafe {
                    nb_msg = libc::sendmmsg(
                        self.socket.as_raw_fd(),
                        self.msgvec.as_mut_ptr(),
                        to_send as u32,
                        0,
                    );
                }
                if nb_msg == -1 {
                    return Err(io::Error::other("libc::sendmmsg"));
                }
                if nb_msg as usize != to_send {
                    log::warn!("nb prepared messages doesn't match with nb sent messages");
                }
            }
        }
        Ok(())
    }
}
