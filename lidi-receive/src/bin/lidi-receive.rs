use lidi_protocol as protocol;
use std::{
    env,
    io::{self, Write},
    net,
    os::{fd::AsRawFd, unix},
    path, thread,
};

enum Client {
    Tcp(net::TcpStream),
    Unix(unix::net::UnixStream),
}

impl Write for Client {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        match self {
            Self::Tcp(socket) => socket.write(buf),
            Self::Unix(socket) => socket.write(buf),
        }
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        match self {
            Self::Tcp(socket) => socket.flush(),
            Self::Unix(socket) => socket.flush(),
        }
    }
}

impl AsRawFd for Client {
    fn as_raw_fd(&self) -> i32 {
        match self {
            Self::Tcp(socket) => socket.as_raw_fd(),
            Self::Unix(socket) => socket.as_raw_fd(),
        }
    }
}

impl TryFrom<&lidi_utils::config::Endpoint> for Client {
    type Error = io::Error;

    fn try_from(endpoint: &lidi_utils::config::Endpoint) -> Result<Self, Self::Error> {
        match endpoint {
            lidi_utils::config::Endpoint::Tcp(to_tcp) => {
                let client = net::TcpStream::connect(to_tcp)?;
                Ok(Self::Tcp(client))
            }
            lidi_utils::config::Endpoint::Unix(to_unix) => {
                let client = unix::net::UnixStream::connect(to_unix)?;
                Ok(Self::Unix(client))
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.len() > 1 {
        eprintln!("too many arguments: expecting only configuration file");
        return;
    }

    let Some(file) = args.first() else {
        eprintln!("missing argument: <config_file>");
        return;
    };

    let config = match lidi_utils::config::parse(path::PathBuf::from(file)) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    if let Err(e) = lidi_utils::init_logger(config.send().log(), false) {
        eprintln!("failed to initialize logger: {e}");
        return;
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let common = config.common();

    let raptorq = match protocol::RaptorQ::new(common.mtu(), common.block(), common.repair()) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let receiver = match lidi_receive::Receiver::new(
        &config,
        raptorq,
        |endpoint, _| Client::try_from(endpoint),
        |_, _| (),
    ) {
        Ok(receiver) => receiver,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    thread::scope(|scope| {
        if let Err(e) = receiver.start(scope) {
            log::error!("failed to start diode receiver: {e}");
        }
    });
}
