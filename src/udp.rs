use std::marker::PhantomData;
use std::os::fd::AsRawFd;
use std::{mem, net};

pub struct UdpRecv;
pub struct UdpSend;

pub struct UdpMessages<D> {
    socket: net::UdpSocket,
    vlen: usize,
    msgvec: Vec<libc::mmsghdr>,
    iovecs: Vec<libc::iovec>,
    buffers: Vec<Vec<u8>>,
    marker: PhantomData<D>,
}

impl<D> UdpMessages<D> {
    fn new(socket: net::UdpSocket, vlen: usize, msglen: Option<usize>) -> Self {
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

        for i in 0..vlen {
            if let Some(msglen) = msglen {
                iovecs[i].iov_base = buffers[i].as_mut_ptr() as *mut libc::c_void;
                iovecs[i].iov_len = msglen;
            }
            msgvec[i].msg_hdr.msg_iov = &mut iovecs[i];
            msgvec[i].msg_hdr.msg_iovlen = 1;
        }

        Self {
            socket,
            vlen,
            msgvec,
            iovecs,
            buffers,
            marker: PhantomData,
        }
    }
}

impl UdpMessages<UdpRecv> {
    pub fn new_receiver(socket: net::UdpSocket, vlen: usize, msglen: usize) -> Self {
        log::info!("UDP configured to receive {vlen} messages (datagrams), of {msglen} bytes each, at a time");
        Self::new(socket, vlen, Some(msglen))
    }

    pub fn recv_mmsg(&mut self) -> impl Iterator<Item = &[u8]> {
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
            log::error!("libc::recvmmsg failed");
            panic!();
        }

        self.buffers
            .iter()
            .take(nb_msg as usize)
            .zip(self.msgvec.iter())
            .map(|(buffer, msghdr)| &buffer[..msghdr.msg_len as usize])
    }
}

impl UdpMessages<UdpSend> {
    pub fn new_sender(socket: net::UdpSocket, vlen: usize) -> UdpMessages<UdpSend> {
        log::info!("UDP configured to send {vlen} messages (datagrams) at a time");
        Self::new(socket, vlen, None)
    }

    pub fn send_mmsg(&mut self, mut buffers: Vec<Vec<u8>>) {
        let mut nb_messages = buffers.len();
        let mut message_offset = 0;

        while nb_messages > 0 {
            let to_send = usize::min(nb_messages, self.vlen);

            for i in 0..to_send {
                self.msgvec[i].msg_len = buffers[i].len() as u32;
                self.iovecs[i].iov_base =
                    buffers[message_offset + i].as_mut_ptr() as *mut libc::c_void;
                self.iovecs[i].iov_len = buffers[i].len();
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
                log::error!("libc::sendmmsg failed");
                panic!();
            }
            if nb_msg as usize != to_send {
                log::warn!("nb prepared messages doesn't match with nb sent messages");
            }

            message_offset += to_send;
            nb_messages -= to_send;
        }
    }
}
