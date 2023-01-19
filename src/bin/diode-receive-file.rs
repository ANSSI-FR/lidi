use clap::{Arg, Command};
use diode::file::protocol;
use log::{debug, error, info};
use std::{
    env, fmt,
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

fn command_args() -> Config {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("from_tcp")
                .long("from_tcp")
                .value_name("ip:port")
                .default_value("127.0.0.1:7000")
                .help("Address and port to listen for diode-receive"),
        )
        .arg(
            Arg::new("buffer_size")
                .long("buffer_size")
                .value_name("nb_bytes")
                .default_value("4194304") // 4096 * 1024
                .value_parser(clap::value_parser!(usize))
                .help("Size of TCP write buffer"),
        )
        .arg(
            Arg::new("output_directory")
                .value_name("dir")
                .default_value(".")
                .help("Output directory"),
        )
        .get_matches();

    let from_tcp = SocketAddr::from_str(args.get_one::<String>("from_tcp").expect("default"))
        .expect("invalid from_tcp parameter");
    let buffer_size = *args.get_one::<usize>("buffer_size").expect("default");
    let output_directory =
        PathBuf::from(args.get_one::<String>("output_directory").expect("default"));

    Config {
        from_tcp,
        buffer_size,
        output_directory,
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
