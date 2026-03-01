use lidi_command_utils::{config, socket};
#[cfg(feature = "receive-mmsg")]
use std::ptr;
use std::{io, net, os::fd::AsRawFd};
#[cfg(any(feature = "receive-msg", feature = "receive-mmsg"))]
use std::{mem, pin};

pub fn get_socket_recv_buffer_size<S: AsRawFd>(socket: &S) -> Result<i32, io::Error> {
    socket::getsockopt_buffer_size(socket.as_raw_fd(), libc::SO_RCVBUF)
}

pub fn set_socket_recv_buffer_size<S: AsRawFd>(socket: &S, size: i32) -> Result<(), io::Error> {
    socket::setsockopt_buffer_size(socket.as_raw_fd(), size, libc::SO_RCVBUF)
}

pub enum ReceiveDatagrams {
    #[cfg(any(feature = "receive-native", feature = "receive-msg"))]
    Single(Vec<u8>),
    #[cfg(feature = "receive-mmsg")]
    Multiple(Vec<Vec<u8>>),
}

#[cfg(feature = "receive-native")]
pub struct ReceiveNative {
    socket: net::UdpSocket,
    udp_packet_size: u16,
    buffer: Vec<u8>,
}

#[cfg(feature = "receive-native")]
impl ReceiveNative {
    fn new(socket: net::UdpSocket, udp_packet_size: u16) -> Self {
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
pub struct ReceiveMsg {
    socket: net::UdpSocket,
    udp_packet_size: u16,
    msghdr: libc::msghdr,
    _iovec: pin::Pin<Box<libc::iovec>>,
    buffer: pin::Pin<Vec<u8>>,
}

#[cfg(feature = "receive-msg")]
impl ReceiveMsg {
    fn new(socket: net::UdpSocket, udp_packet_size: u16) -> Self {
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

    fn recv(&mut self) -> Result<ReceiveDatagrams, io::Error> {
        let recv = unsafe { libc::recvmsg(self.socket.as_raw_fd(), &raw mut self.msghdr, 0) };

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
pub struct ReceiveMmsg {
    socket: net::UdpSocket,
    mmsghdr: Vec<libc::mmsghdr>,
    _iovecs: pin::Pin<Vec<libc::iovec>>,
    buffers: Vec<pin::Pin<Vec<u8>>>,
}

#[cfg(feature = "receive-mmsg")]
impl ReceiveMmsg {
    fn new(socket: net::UdpSocket, udp_packet_size: u16) -> Self {
        let iovecs =
            vec![unsafe { mem::zeroed::<libc::iovec>() }; socket::MAX_MMSG_BATCH_SIZE as usize];
        let mut iovecs = pin::Pin::new(iovecs);

        let mut mmsghdr =
            vec![unsafe { mem::zeroed::<libc::mmsghdr>() }; socket::MAX_MMSG_BATCH_SIZE as usize];
        for i in 0..socket::MAX_MMSG_BATCH_SIZE as usize {
            mmsghdr[i].msg_hdr.msg_iov = &raw mut iovecs[i];
            mmsghdr[i].msg_hdr.msg_iovlen = 1;
        }

        let mut buffers = vec![
            pin::Pin::new(vec![0u8; udp_packet_size as usize]);
            socket::MAX_MMSG_BATCH_SIZE as usize
        ];

        for (i, buffer) in buffers.iter_mut().enumerate() {
            iovecs[i].iov_base = buffer.as_mut_ptr().cast::<libc::c_void>();
            iovecs[i].iov_len = udp_packet_size as usize;
        }

        Self {
            socket,
            mmsghdr,
            _iovecs: iovecs,
            buffers,
        }
    }

    fn recv(&mut self) -> Result<ReceiveDatagrams, io::Error> {
        let nb_msg = unsafe {
            libc::recvmmsg(
                self.socket.as_raw_fd(),
                self.mmsghdr.as_mut_ptr(),
                socket::MAX_MMSG_BATCH_SIZE,
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

pub enum Receive {
    #[cfg(feature = "receive-native")]
    Native(ReceiveNative),
    #[cfg(feature = "receive-msg")]
    Msg(ReceiveMsg),
    #[cfg(feature = "receive-mmsg")]
    Mmsg(ReceiveMmsg),
}

impl Receive {
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(
        socket: net::UdpSocket,
        udp_packet_size: u16,
        mode: config::Mode,
    ) -> Result<Self, io::Error> {
        match mode {
            config::Mode::Native => {
                #[cfg(not(feature = "receive-native"))]
                {
                    Err(io::Error::new(io::ErrorKind::Unsupported, "receive-native"))
                }
                #[cfg(feature = "receive-native")]
                {
                    Ok(Self::Native(ReceiveNative::new(socket, udp_packet_size)))
                }
            }
            config::Mode::Msg => {
                #[cfg(not(feature = "receive-msg"))]
                {
                    Err(io::Error::new(io::ErrorKind::Unsupported, "receive-msg"))
                }
                #[cfg(feature = "receive-msg")]
                {
                    Ok(Self::Msg(ReceiveMsg::new(socket, udp_packet_size)))
                }
            }
            config::Mode::Mmsg => {
                #[cfg(not(feature = "receive-mmsg"))]
                {
                    Err(io::Error::new(io::ErrorKind::Unsupported, "receive-mmsg"))
                }
                #[cfg(feature = "receive-mmsg")]
                {
                    Ok(Self::Mmsg(ReceiveMmsg::new(socket, udp_packet_size)))
                }
            }
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
