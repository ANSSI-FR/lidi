//! Functions and wrappers over libc's UDP socket multiple messages receive and send

use std::{io, mem, net, num, pin, ptr};

pub(crate) enum Datagrams {
    Single(Vec<u8>),
    Multiple(Vec<Vec<u8>>),
}

pub(crate) struct ReceiveMsg {
    socket: i32,
    udp_packet_size: u16,
    msghdr: libc::msghdr,
    _iovec: pin::Pin<Box<libc::iovec>>,
    buffer: pin::Pin<Vec<u8>>,
}

impl ReceiveMsg {
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
        }
    }

    fn recv(&mut self) -> Result<Datagrams, io::Error> {
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

        Ok(Datagrams::Single(self.buffer[0..recv].to_vec()))
    }
}

pub(crate) struct ReceiveMmsg {
    socket: i32,
    mmsghdr: Vec<libc::mmsghdr>,
    _iovecs: pin::Pin<Vec<libc::iovec>>,
    buffers: Vec<pin::Pin<Vec<u8>>>,
    batch_size: u32,
}

impl ReceiveMmsg {
    fn new(socket: i32, udp_packet_size: u16, batch_size: u32) -> Self {
        let iovecs = vec![unsafe { mem::zeroed::<libc::iovec>() }; batch_size as usize];
        let mut iovecs = pin::Pin::new(iovecs);

        let mut mmsghdr = vec![unsafe { mem::zeroed::<libc::mmsghdr>() }; batch_size as usize];
        for i in 0..batch_size as usize {
            mmsghdr[i].msg_hdr.msg_iov = &raw mut iovecs[i];
            mmsghdr[i].msg_hdr.msg_iovlen = 1;
        }

        let mut buffers =
            vec![pin::Pin::new(vec![0u8; udp_packet_size as usize]); batch_size as usize];

        for (i, buffer) in buffers.iter_mut().enumerate() {
            iovecs[i].iov_base = buffer.as_mut_ptr().cast::<libc::c_void>();
            iovecs[i].iov_len = udp_packet_size as usize;
        }

        Self {
            socket,
            mmsghdr,
            _iovecs: iovecs,
            buffers,
            batch_size,
        }
    }

    fn recv(&mut self) -> Result<Datagrams, io::Error> {
        let nb_msg = unsafe {
            libc::recvmmsg(
                self.socket,
                self.mmsghdr.as_mut_ptr(),
                self.batch_size,
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

            Ok(Datagrams::Multiple(buffers))
        }
    }
}

pub(crate) enum Receive {
    Msg(ReceiveMsg),
    Mmsg(ReceiveMmsg),
}

impl Receive {
    pub(crate) fn new(socket: i32, udp_packet_size: u16, batch_receive: Option<u32>) -> Self {
        match batch_receive {
            None | Some(1) => Self::Msg(ReceiveMsg::new(socket, udp_packet_size)),
            Some(n) => Self::Mmsg(ReceiveMmsg::new(socket, udp_packet_size, n)),
        }
    }

    pub(crate) fn recv(&mut self) -> Result<Datagrams, io::Error> {
        match self {
            Self::Msg(receiver) => receiver.recv(),
            Self::Mmsg(receiver) => receiver.recv(),
        }
    }
}

enum SendM {
    Msg {
        socket: i32,
        msghdr: libc::msghdr,
        iovec: pin::Pin<Box<libc::iovec>>,
    },
    Mmsg {
        socket: i32,
        batch_size: usize,
        mmsghdr: Vec<libc::mmsghdr>,
        iovecs: pin::Pin<Vec<libc::iovec>>,
    },
}

impl SendM {
    fn new(
        batch_send: Option<u32>,
        socket: i32,
        dest: *mut libc::sockaddr,
        dest_len: u32,
    ) -> Result<Self, io::Error> {
        match batch_send {
            None | Some(1) => {
                let iovec = unsafe { mem::zeroed::<libc::iovec>() };
                let mut iovec = pin::Pin::new(Box::new(iovec));

                let mut msghdr = unsafe { mem::zeroed::<libc::msghdr>() };

                msghdr.msg_name = dest.cast::<libc::c_void>();
                msghdr.msg_namelen = dest_len;
                msghdr.msg_iov = &raw mut *iovec;
                msghdr.msg_iovlen = 1;

                Ok(Self::Msg {
                    socket,
                    msghdr,
                    iovec,
                })
            }
            Some(batch_size) => {
                let batch_size = usize::try_from(batch_size).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("batch_size: {e}"))
                })?;
                let iovecs = vec![unsafe { mem::zeroed::<libc::iovec>() }; batch_size];
                let mut iovecs = pin::Pin::new(iovecs);

                let mut mmsghdr = vec![unsafe { mem::zeroed::<libc::mmsghdr>() }; batch_size];

                for i in 0..batch_size {
                    mmsghdr[i].msg_hdr.msg_name = dest.cast::<libc::c_void>();
                    mmsghdr[i].msg_hdr.msg_namelen = dest_len;
                    mmsghdr[i].msg_hdr.msg_iov = &raw mut iovecs[i];
                    mmsghdr[i].msg_hdr.msg_iovlen = 1;
                }

                Ok(Self::Mmsg {
                    socket,
                    mmsghdr,
                    iovecs,
                    batch_size,
                })
            }
        }
    }

    fn send(&mut self, packets: Vec<raptorq::EncodingPacket>) -> Result<(), io::Error> {
        let mut datagrams = packets.into_iter().map(|packet| packet.serialize());

        match self {
            Self::Msg {
                socket,
                msghdr,
                iovec,
            } => datagrams.try_for_each(|mut datagram| {
                let len = datagram.len();

                iovec.iov_base = datagram.as_mut_ptr().cast();
                iovec.iov_len = len;

                let sent = unsafe { libc::sendmsg(*socket, msghdr, 0) };

                if sent == len.cast_signed() {
                    Ok(())
                } else {
                    Err(io::Error::other(format!(
                        "libc::sendmsg failed {sent} != {len}"
                    )))
                }
            }),
            Self::Mmsg {
                socket,
                batch_size,
                mmsghdr,
                iovecs,
            } => datagrams
                .collect::<Vec<_>>()
                .chunks_mut(*batch_size)
                .try_for_each(|datagrams| {
                    let to_send = datagrams.len();

                    datagrams
                        .iter_mut()
                        .enumerate()
                        .try_for_each(|(i, datagram)| {
                            mmsghdr[i].msg_len = u32::try_from(datagram.len())?;
                            iovecs[i].iov_base = datagram.as_mut_ptr().cast::<libc::c_void>();
                            iovecs[i].iov_len = datagram.len();
                            Ok(())
                        })
                        .map_err(|e: num::TryFromIntError| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("datagram.len(): {e}"),
                            )
                        })?;

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

                    if sent.cast_unsigned() == to_send {
                        Ok(())
                    } else {
                        Err(io::Error::other("libc::sendmmsg"))
                    }
                }),
        }
    }
}

pub(crate) struct Send {
    _dest: pin::Pin<Box<libc::sockaddr>>,
    sendm: SendM,
}

impl Send {
    pub(crate) fn new(
        socket: i32,
        dest: net::SocketAddr,
        batch_send: Option<u32>,
    ) -> Result<Self, io::Error> {
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
        let mut dest: pin::Pin<Box<libc::sockaddr>> = pin::Pin::new(dest);
        let sendm = SendM::new(
            batch_send,
            socket,
            (&raw mut *dest).cast::<libc::sockaddr>(),
            u32::try_from(dest_len).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("dest_len: {e}"))
            })?,
        )?;

        Ok(Self { _dest: dest, sendm })
    }

    pub(crate) fn send(
        &mut self,
        datagrams: Vec<raptorq::EncodingPacket>,
    ) -> Result<(), io::Error> {
        self.sendm.send(datagrams)
    }
}
