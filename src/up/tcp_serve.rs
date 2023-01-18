use crossbeam_channel::{self, Receiver, RecvTimeoutError};
use log::{error, info, trace, warn};
use std::{
    fmt,
    io::{self, Write},
    net::{self, TcpStream},
    time::Duration,
};

#[derive(Clone)]
pub(crate) struct Config {
    pub to_tcp: net::SocketAddr,
    pub to_tcp_buffer_size: usize,
    pub abort_timeout: u64,
}

pub(crate) enum Error {
    Io(io::Error),
    Crossbeam(RecvTimeoutError),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Crossbeam(e) => write!(fmt, "crossbeam send error: {e}"),
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

pub(crate) fn new(
    config: Config,
    client_id: protocol::ClientId,
    recvq: Receiver<protocol::Message>,
) {
    if let Err(e) = main_loop(config, client_id, recvq) {
        error!("client {client_id:x}: TCP send loop error: {e}");
    }
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
        match recvq.recv_timeout(Duration::from_secs(config.abort_timeout)) {
            Err(RecvTimeoutError::Timeout) => {
                warn!("client {client_id:x}: transfer tiemout, aborting");
                return Err(Error::from(RecvTimeoutError::Timeout));
            }
            Err(e) => return Err(Error::from(e)),
            Ok(message) => {
                match message {
                    protocol::Message::Data(data) => {
                        trace!("client {client_id:x}: transfer {} bytes", data.len());
                        transmitted += data.len();
                        client.write_all(&data)?;
                    }
                    protocol::Message::Abort => {
                        warn!("client {client_id:x}: aborting transfer");
                        return Ok(());
                    }
                    protocol::Message::End => {
                        info!("client {client_id:x}: finished transfer, {transmitted} bytes transmitted");
                        client.flush()?;
                        return Ok(());
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}
