use crate::{protocol, semaphore};
use crossbeam_channel::{self, Receiver, RecvTimeoutError};
use log::{debug, error, info, trace, warn};
use std::{
    fmt,
    io::{self, Write},
    net::{self, TcpStream},
    time::Duration,
};

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) to_tcp: net::SocketAddr,
    pub(crate) to_tcp_buffer_size: usize,
    pub(crate) abort_timeout: Duration,
}

enum Error {
    Io(io::Error),
    Crossbeam(RecvTimeoutError),
    Diode(protocol::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Crossbeam(e) => write!(fmt, "crossbeam send error: {e}"),
            Self::Diode(e) => write!(fmt, "diode error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<RecvTimeoutError> for Error {
    fn from(e: RecvTimeoutError) -> Self {
        Self::Crossbeam(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Diode(e)
    }
}

pub(crate) fn new(
    config: Config,
    multiplex_control: semaphore::Semaphore,
    client_id: protocol::ClientId,
    recvq: Receiver<protocol::Message>,
) {
    debug!("try to acquire multiplex access..");

    multiplex_control.acquire();

    debug!("multiplex access acquired");

    if let Err(e) = main_loop(config, client_id, recvq) {
        error!("client {client_id:x}: TCP send loop error: {e}");
    }

    multiplex_control.release()
}

fn main_loop(
    config: Config,
    client_id: protocol::ClientId,
    recvq: Receiver<protocol::Message>,
) -> Result<(), Error> {
    info!("client {client_id:x}: starting transfer");

    let socket = TcpStream::connect(config.to_tcp)?;

    socket.shutdown(net::Shutdown::Read)?;

    let mut client = io::BufWriter::with_capacity(config.to_tcp_buffer_size, socket);

    let mut transmitted = 0;

    loop {
        match recvq.recv_timeout(config.abort_timeout) {
            Err(RecvTimeoutError::Timeout) => {
                warn!("client {client_id:x}: transfer tiemout, aborting");
                return Err(Error::from(RecvTimeoutError::Timeout));
            }
            Err(e) => return Err(Error::from(e)),
            Ok(message) => {
                let message_type = message.message_type()?;

                let payload = message.payload();

                if !payload.is_empty() {
                    trace!("client {client_id:x}: payload {} bytes", payload.len());
                    transmitted += payload.len();
                    client.write_all(payload)?;
                }

                match message_type {
                    protocol::MessageType::Abort => {
                        warn!("client {client_id:x}: aborting transfer");
                        return Ok(());
                    }
                    protocol::MessageType::End => {
                        info!("client {client_id:x}: finished transfer, {transmitted} bytes transmitted");
                        client.flush()?;
                        return Ok(());
                    }
                    _ => (),
                }
            }
        }
    }
}
