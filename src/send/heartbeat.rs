use crossbeam_channel::{tick, RecvError, SendError, Sender};
use log::error;
use std::{fmt, time::Duration};

use crate::protocol;

enum Error {
    Receive(RecvError),
    Send(SendError<protocol::Message>),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::Send(e) => write!(fmt, "crossbeam send error: {e}"),
        }
    }
}

impl From<RecvError> for Error {
    fn from(e: RecvError) -> Self {
        Self::Receive(e)
    }
}

impl From<SendError<protocol::Message>> for Error {
    fn from(e: SendError<protocol::Message>) -> Self {
        Self::Send(e)
    }
}

pub struct Config {
    pub buffer_size: u32,
    pub duration: Duration,
}

pub fn new(config: &Config, sendq: &Sender<protocol::Message>) {
    if let Err(e) = main_loop(config, sendq) {
        error!("heartbeat loop error: {e}");
    }
}

fn main_loop(config: &Config, sendq: &Sender<protocol::Message>) -> Result<(), Error> {
    let alarm = tick(config.duration);

    loop {
        sendq.send(protocol::Message::new(
            protocol::MessageType::Heartbeat,
            config.buffer_size,
            0,
            None,
        ))?;
        let _ = alarm.recv()?;
    }
}
