use diode::file::protocol;

use clap::{Arg, ArgAction, Command};
use log::{debug, error, info};
use std::{
    env, fmt,
    fs::OpenOptions,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::prelude::PermissionsExt,
    path::PathBuf,
    str::FromStr,
};

struct Config {
    to_tcp: SocketAddr,
    buffer_size: usize,
    files: Vec<String>,
}

fn command_args() -> Config {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("to_tcp")
                .long("to_tcp")
                .value_name("ip:port")
                .default_value("127.0.0.1:5000")
                .help("Address and port to connect to diode-down"),
        )
        .arg(
            Arg::new("buffer_size")
                .long("buffer_size")
                .value_name("nb_bytes")
                .default_value("4194304") // 4096 * 1024
                .value_parser(clap::value_parser!(usize))
                .help("Size of file read/TCP write buffer"),
        )
        .arg(
            Arg::new("file")
                .action(ArgAction::Append)
                .allow_hyphen_values(true)
                .required(true),
        )
        .get_matches();

    let to_tcp = SocketAddr::from_str(args.get_one::<String>("to_tcp").expect("default"))
        .expect("invalid to_tcp parameter");
    let buffer_size = *args.get_one::<usize>("buffer_size").expect("default");
    let files = args.get_many("file").expect("required").cloned().collect();

    Config {
        to_tcp,
        buffer_size,
        files,
    }
}

enum Error {
    Io(io::Error),
    Diode(protocol::Error),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::Diode(e) => write!(fmt, "diode error: {e}"),
            Self::Other(e) => write!(fmt, "error: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Self::Diode(e)
    }
}

fn file_loop(config: &Config, file_path: &String) -> Result<usize, Error> {
    debug!("opening file \"{}\"", file_path);

    let file_path = PathBuf::from(file_path);

    if !file_path.is_file() {
        return Err(Error::Other("not a file".to_string()));
    }

    let mut file = OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(&file_path)?;

    let file_name = file_path
        .file_name()
        .ok_or(Error::Other("unwrap of file_name failed".to_string()))?
        .to_os_string()
        .into_string()
        .map_err(|_| Error::Other("conversion from OsString to String failed".to_string()))?;

    debug!("file name is \"{file_name}\"");

    debug!("connecting to {}", config.to_tcp);

    let mut diode = TcpStream::connect(config.to_tcp)?;

    diode.shutdown(std::net::Shutdown::Read)?;

    let metadata = file.metadata()?;
    let permissions = metadata.permissions();

    let header = protocol::Header {
        file_name,
        mode: permissions.mode(),
        file_length: metadata.len(),
    };

    header.serialize_to(&mut diode)?;

    let mut buffer = vec![0; config.buffer_size];
    let mut cursor = 0;
    let mut total = 0;

    loop {
        match file.read(&mut buffer[cursor..])? {
            0 => {
                if 0 < cursor {
                    total += cursor;
                    diode.write_all(&buffer[..cursor])?;
                }
                diode.flush()?;
                return Ok(total);
            }
            nread => {
                if (cursor + nread) < config.buffer_size {
                    cursor += nread;
                    continue;
                }
                total += config.buffer_size;
                diode.write_all(&buffer)?;
                cursor = 0;
            }
        }
    }
}

fn main_loop(config: Config) -> Result<(), Error> {
    for file in &config.files {
        let total = file_loop(&config, file)?;
        info!("file send, {total} bytes sent");
    }
    Ok(())
}

fn main() {
    let config = command_args();

    init_logger();

    if let Err(e) = main_loop(config) {
        error!("{e}");
    }
}

fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env().unwrap();
    } else {
        simple_logger::init_with_level(log::Level::Info).unwrap();
    }
}
