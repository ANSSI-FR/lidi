//! Functions and wrappers over libc's UDP socket multiple messages receive and send

use super::{RecvMode, SendMode};
#[cfg(feature = "send-mmsg")]
use std::num;
#[cfg(feature = "receive-mmsg")]
use std::ptr;
use std::{io, net};
#[cfg(any(
    feature = "receive-msg",
    feature = "receive-mmsg",
    feature = "send-msg",
    feature = "send-mmsg"
))]
use std::{marker, mem, os::fd::AsRawFd, pin};

#[cfg(any(feature = "receive-mmsg", feature = "send-mmsg"))]
const MAX_BATCH_SIZE: u32 = 1024;

pub enum ReceiveDatagrams {
    #[cfg(any(feature = "receive-native", feature = "receive-msg"))]
    Single(Vec<u8>),
    #[cfg(feature = "receive-mmsg")]
    Multiple(Vec<Vec<u8>>),
}

#[cfg(feature = "receive-native")]
pub struct ReceiveNative<'a> {
    socket: &'a mut net::UdpSocket,
    udp_packet_size: u16,
    buffer: Vec<u8>,
}

#[cfg(feature = "receive-native")]
impl<'a> ReceiveNative<'a> {
    fn new(socket: &'a mut net::UdpSocket, udp_packet_size: u16) -> Self {
        let buffer = vec![0u8; udp_packet_size as usize];

        Self {
            socket,
            udp_packet_size,
            buffer,
        }
    }

    fn recv(&mut self) -> Result<ReceiveDatagrams, io::Error> {
        let recv = self.socket.recv(&mut self.buffer)?;

        if recv == 0 {
            return Err(io::Error::other(format!(
                "recv {recv} != {}",
                self.udp_packet_size
            )));
        }

        Ok(ReceiveDatagrams::Single(self.buffer[0..recv].to_vec()))
    }
}

#[cfg(feature = "receive-msg")]
pub struct ReceiveMsg<'a> {
    socket: i32,
    udp_packet_size: u16,
    msghdr: libc::msghdr,
    _iovec: pin::Pin<Box<libc::iovec>>,
    buffer: pin::Pin<Vec<u8>>,
    phantom: marker::PhantomData<&'a ()>,
}

#[cfg(feature = "receive-msg")]
impl ReceiveMsg<'_> {
    fn new(socket: i32, udp_packet_size: u16) -> Self {
        let iovec = unsafe { mem::zeroed::<libc::iovec>() };
        let mut iovec = pin::Pin::new(Box::new(iovec));

        let mut msghdr = unsafe { mem::zeroed::<libc::msghdr>() };
        msghdr.msg_iov = &raw mut *iovec;
        msghdr.msg_iovlen = 1;

        let mut buffer = pin::Pin::new(vec![0u8; udp_packet_size as usize]);

        iovec.iov_base = buffer.as_mut_ptr().cast::<libc::c_void>();
        iovec.iov_len = udp_packet_size as usize;

        Self {
            socket,
            udp_packet_size,
            msghdr,
            _iovec: iovec,
            buffer,
            phantom: marker::PhantomData,
        }
    }

    fn recv(&mut self) -> Result<ReceiveDatagrams, io::Error> {
        let recv = unsafe { libc::recvmsg(self.socket, &raw mut self.msghdr, 0) };

        if recv < 0 {
            let errno = unsafe { *libc::__errno_location() };
            return Err(io::Error::other(format!(
                "libc::recvmsg {recv} != {}, (errno == {errno})",
                self.udp_packet_size
            )));
        }

        let recv = usize::try_from(recv)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("recv: {e}")))?;

        Ok(ReceiveDatagrams::Single(self.buffer[0..recv].to_vec()))
    }
}

#[cfg(feature = "receive-mmsg")]
pub struct ReceiveMmsg<'a> {
    socket: i32,
    mmsghdr: Vec<libc::mmsghdr>,
    _iovecs: pin::Pin<Vec<libc::iovec>>,
    buffers: Vec<pin::Pin<Vec<u8>>>,
    phantom: marker::PhantomData<&'a ()>,
}

#[cfg(feature = "receive-mmsg")]
impl ReceiveMmsg<'_> {
    fn new(socket: i32, udp_packet_size: u16) -> Self {
        let iovecs = vec![unsafe { mem::zeroed::<libc::iovec>() }; MAX_BATCH_SIZE as usize];
        let mut iovecs = pin::Pin::new(iovecs);

        let mut mmsghdr = vec![unsafe { mem::zeroed::<libc::mmsghdr>() }; MAX_BATCH_SIZE as usize];
        for i in 0..MAX_BATCH_SIZE as usize {
            mmsghdr[i].msg_hdr.msg_iov = &raw mut iovecs[i];
            mmsghdr[i].msg_hdr.msg_iovlen = 1;
        }

        let mut buffers = vec![pin::Pin::new(vec![0u8; udp_packet_size as usize]); MAX_BATCH_SIZE as usize];

        for (i, buffer) in buffers.iter_mut().enumerate() {
            iovecs[i].iov_base = buffer.as_mut_ptr().cast::<libc::c_void>();
            iovecs[i].iov_len = udp_packet_size as usize;
        }

        Self {
            socket,
            mmsghdr,
            _iovecs: iovecs,
            buffers,
            phantom: marker::PhantomData,
        }
    }

    fn recv(&mut self) -> Result<ReceiveDatagrams, io::Error> {
        let nb_msg = unsafe {
            libc::recvmmsg(
                self.socket,
                self.mmsghdr.as_mut_ptr(),
                MAX_BATCH_SIZE,
                libc::MSG_WAITFORONE,
                ptr::null_mut(),
            )
        };

        if nb_msg == -1 {
            let errno = unsafe { *libc::__errno_location() };
            Err(io::Error::other(format!("libc::recvmmsg, errno = {errno}")))
        } else {
            let nb_msg = usize::try_from(nb_msg)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("nb_msg: {e}")))?;

            let buffers = self.buffers[0..nb_msg].iter().enumerate().try_fold(
                Vec::with_capacity(nb_msg),
                |mut res, (i, buffer)| {
                    let msg_len = usize::try_from(self.mmsghdr[i].msg_len).map_err(|e| {
                        io::Error::new(io::ErrorKind::InvalidData, format!("msg_len: {e}"))
                    })?;
                    res.push(buffer[0..msg_len].to_vec());
                    Ok::<_, io::Error>(res)
                },
            )?;

            Ok(ReceiveDatagrams::Multiple(buffers))
        }
    }
}

pub enum Receive<'a> {
    #[cfg(feature = "receive-native")]
    Native(ReceiveNative<'a>),
    #[cfg(feature = "receive-msg")]
    Msg(ReceiveMsg<'a>),
    #[cfg(feature = "receive-mmsg")]
    Mmsg(ReceiveMmsg<'a>),
}

impl<'a> Receive<'a> {
    pub fn new(socket: &'a mut net::UdpSocket, udp_packet_size: u16, mode: RecvMode) -> Self {
        match mode {
            #[cfg(feature = "receive-native")]
            RecvMode::Native => Self::Native(ReceiveNative::new(socket, udp_packet_size)),
            #[cfg(feature = "receive-msg")]
            RecvMode::Recvmsg => Self::Msg(ReceiveMsg::new(socket.as_raw_fd(), udp_packet_size)),
            #[cfg(feature = "receive-mmsg")]
            RecvMode::Recvmmsg => Self::Mmsg(ReceiveMmsg::new(socket.as_raw_fd(), udp_packet_size)),
        }
    }

    pub fn recv(&mut self) -> Result<ReceiveDatagrams, io::Error> {
        match self {
            #[cfg(feature = "receive-native")]
            Self::Native(receiver) => receiver.recv(),
            #[cfg(feature = "receive-msg")]
            Self::Msg(receiver) => receiver.recv(),
            #[cfg(feature = "receive-mmsg")]
            Self::Mmsg(receiver) => receiver.recv(),
        }
    }
}

pub enum Send<'a> {
    #[cfg(feature = "send-native")]
    Native {
        socket: &'a mut net::UdpSocket,
        dest: net::SocketAddr,
    },
    #[cfg(feature = "send-msg")]
    Msg {
        socket: i32,
        _dest: pin::Pin<Box<libc::sockaddr>>,
        msghdr: libc::msghdr,
        iovec: pin::Pin<Box<libc::iovec>>,
        phantom: marker::PhantomData<&'a ()>,
    },
    #[cfg(feature = "send-mmsg")]
    Mmsg {
        socket: i32,
        _dest: pin::Pin<Box<libc::sockaddr>>,
        mmsghdr: Vec<libc::mmsghdr>,
        iovecs: pin::Pin<Vec<libc::iovec>>,
        phantom: marker::PhantomData<&'a ()>,
    },
}

#[cfg(any(feature = "send-msg", feature = "send-mmsg"))]
fn convert_address(
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

impl<'a> Send<'a> {
    pub fn new(
        socket: &'a mut net::UdpSocket,
        dest: net::SocketAddr,
        mode: SendMode,
    ) -> Result<Self, io::Error> {
        match mode {
            #[cfg(feature = "send-native")]
            SendMode::Native => Ok(Self::Native { socket, dest }),
            #[cfg(feature = "send-msg")]
            SendMode::Sendmsg => {
                let socket = socket.as_raw_fd();
                let (mut dest, dest_len) = convert_address(dest)?;
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
                    phantom: marker::PhantomData,
                })
            }
            #[cfg(feature = "send-mmsg")]
            SendMode::Sendmmsg => {
                let socket = socket.as_raw_fd();
                let (mut dest, dest_len) = convert_address(dest)?;
                let raw_dest = (&raw mut *dest).cast::<libc::sockaddr>();
                let dest_len = u32::try_from(dest_len).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("dest_len: {e}"))
                })?;

                let iovecs = vec![unsafe { mem::zeroed::<libc::iovec>() }; MAX_BATCH_SIZE as usize];
                let mut iovecs = pin::Pin::new(iovecs);

                let mut mmsghdr = vec![unsafe { mem::zeroed::<libc::mmsghdr>() }; MAX_BATCH_SIZE as usize];

                for i in 0..MAX_BATCH_SIZE as usize {
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
                    phantom: marker::PhantomData,
                })
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

                    let sent = unsafe { libc::sendmsg(*socket, msghdr, 0) };

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
                    .chunks_mut(MAX_BATCH_SIZE as usize)
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
                            *socket,
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
