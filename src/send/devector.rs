use crossbeam_channel::{Receiver, RecvError, SendError, Sender};
use log::error;
use std::{fmt, io};

use super::udp_send;

enum Error {
    Io(io::Error),
    Receive(RecvError),
    Send(SendError<udp_send::Message>),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::Send(e) => write!(fmt, "crossbeam send error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<RecvError> for Error {
    fn from(e: RecvError) -> Self {
        Self::Receive(e)
    }
}

impl From<SendError<udp_send::Message>> for Error {
    fn from(e: SendError<udp_send::Message>) -> Self {
        Self::Send(e)
    }
}

pub type Message = Vec<udp_send::Message>;

pub fn new(recvq: Receiver<Message>, sendq: Sender<udp_send::Message>) {
    if let Err(e) = main_loop(recvq, sendq) {
        error!("devector send loop error: {e}");
    }
}

fn main_loop(recvq: Receiver<Message>, sendq: Sender<udp_send::Message>) -> Result<(), Error> {
    loop {
        let packets = recvq.recv()?;
        for packet in packets.into_iter() {
            sendq.send(packet)?;
        }
    }
}
