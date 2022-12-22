// SPDX-License-Identifier: LGPL-3.0

use nix::{
    cmsg_space,
    sys::socket::{
        bind, recvmsg, sendmsg, socket, AddressFamily, ControlMessage, ControlMessageOwned,
        MsgFlags, SockFlag, SockProtocol, SockType, SockaddrLike, SockaddrStorage,
    },
    Error,
};
use std::{io::IoSlice, io::IoSliceMut, net::SocketAddr, os::unix::io::RawFd};

pub fn bind_udp_socket(addr: SocketAddr) -> Result<RawFd, Error> {
    let sock = socket(
        AddressFamily::Inet,
        SockType::Datagram,
        SockFlag::empty(),
        SockProtocol::Udp,
    )?;

    let sockaddr = SockaddrStorage::from(addr);
    bind(sock, &sockaddr)?;

    Ok(sock)
}

#[allow(dead_code)]
pub fn sendfd(socket: RawFd, buffer: &[u8], fd: RawFd) -> Result<usize, Error> {
    let fds = [fd];
    let cmsg = ControlMessage::ScmRights(&fds);

    sendmsg::<()>(
        socket,
        &[IoSlice::new(buffer)],
        &[cmsg],
        MsgFlags::empty(),
        None,
    )
}

pub fn recvfd(socket: RawFd, buffer: &mut [u8]) -> Result<(Option<RawFd>, isize), Error> {
    let mut cmsgspace = cmsg_space!(RawFd);
    let msg = recvmsg::<()>(
        socket,
        &mut [IoSliceMut::new(buffer)],
        Some(&mut cmsgspace),
        MsgFlags::empty(),
    )?;

    for cmsg in msg.cmsgs() {
        if let ControlMessageOwned::ScmRights(fd) = cmsg {
            assert_eq!(fd.len(), 1);
            return Ok((Some(fd[0]), msg.bytes as isize));
        }
    }

    Ok((None, msg.bytes as isize))
}
