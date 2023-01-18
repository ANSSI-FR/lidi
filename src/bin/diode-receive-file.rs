use diode::file::protocol;

use clap::{Arg, ArgAction, Command};
use log::{debug, error, info};
use std::{
    fmt,
    fs::{OpenOptions, Permissions},
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    str::FromStr,
    thread,
};

#[derive(Clone)]
struct Config {
    from_tcp: SocketAddr,
    buffer_size: usize,
    output_directory: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            from_tcp: SocketAddr::from_str("127.0.0.1:7000").unwrap(),
            buffer_size: 4096 * 1024,
            output_directory: PathBuf::from("."),
        }
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

fn command_args(config: &mut Config) {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_tcp")
                .long("from_tcp")
                .action(ArgAction::Set)
                .value_name("ip:port")
                .help("Address and port to listen for diode-up"),
        )
        .arg(
            Arg::new("buffer_size")
                .long("buffer_size")
                .action(ArgAction::Set)
                .value_name("nb_bytes")
                .value_parser(clap::value_parser!(usize))
                .help("Size of TCP write buffer"),
        )
        .arg(
            Arg::new("output_directory")
                .required(true)
                .action(ArgAction::Set)
                .value_name("dir")
                .value_parser(clap::value_parser!(String))
                .help("Output directory"),
        )
        .get_matches();

    if let Some(p) = args.get_one::<String>("from_tcp") {
        let p = SocketAddr::from_str(p).expect("invalid from_tcp parameter");
        config.from_tcp = p;
    }

    if let Some(p) = args.get_one::<usize>("buffer_size") {
        config.buffer_size = *p;
    }

    if let Some(p) = args.get_one::<String>("output_directory") {
        config.output_directory = PathBuf::from(&p);
    }
}

fn client_main_loop_aux(config: Config, mut diode: TcpStream) -> Result<usize, Error> {
    info!("new client connected");

    diode.shutdown(std::net::Shutdown::Write)?;

    let header = protocol::Header::deserialize_from(&mut diode)?;

    debug!("receiving file \"{}\"", header.file_name);

    let file_path = PathBuf::from(header.file_name);
    let file_name = file_path
        .file_name()
        .ok_or(Error::Other("unwrap of file_name failed".to_string()))?;
    let file_path = config.output_directory.join(PathBuf::from(file_name));

    debug!("storing at \"{}\"", file_path.display());

    if file_path.exists() {
        return Err(Error::Other(format!(
            "file \"{}\" already exists",
            file_path.display()
        )));
    }

    let mut file = OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .open(&file_path)?;

    debug!("setting mode to {}", header.mode);
    file.set_permissions(Permissions::from_mode(header.mode))?;

    let mut buffer = vec![0; config.buffer_size];
    let mut cursor = 0;
    let mut total = 0;

    loop {
        match diode.read(&mut buffer[cursor..])? {
            0 => {
                if 0 < cursor {
                    total += cursor;
                    file.write_all(&buffer[..cursor])?;
                }
                file.flush()?;
                return Ok(total);
            }
            nread => {
                if (cursor + nread) < config.buffer_size {
                    cursor += nread;
                    continue;
                }
                total += config.buffer_size;
                file.write_all(&buffer)?;
                cursor = 0;
            }
        }
    }
}

fn client_main_loop(config: Config, client: TcpStream) {
    match client_main_loop_aux(config, client) {
        Err(e) => error!("{e}"),
        Ok(total) => info!("file received, {total} bytes received"),
    }
}

fn main_loop(config: Config) -> Result<(), Error> {
    if !config.output_directory.is_dir() {
        return Err(Error::Other(
            "output_directory is not a directory".to_string(),
        ));
    }

    let server = TcpListener::bind(config.from_tcp)?;

    for client in server.incoming() {
        let config = config.clone();
        let client = client?;
        thread::spawn(move || client_main_loop(config, client));
    }

    Ok(())
}

fn main() {
    let mut config = Config::default();

    command_args(&mut config);

    protocol::init_logger();

    if let Err(e) = main_loop(config) {
        error!("{e}");
    }
}
