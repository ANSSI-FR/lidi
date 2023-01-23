use crate::file::{self, protocol};
use log::{debug, info};
use std::{
    fs::{OpenOptions, Permissions},
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
};

pub struct Config {
    pub from_tcp: SocketAddr,
    pub buffer_size: usize,
    pub output_directory: PathBuf,
}

pub fn receive_file(config: &Config, mut diode: TcpStream) -> Result<usize, file::Error> {
    info!("new client connected");

    diode.shutdown(std::net::Shutdown::Write)?;

    let header = protocol::Header::deserialize_from(&mut diode)?;

    debug!("receiving file \"{}\"", header.file_name);

    let file_path = PathBuf::from(header.file_name);
    let file_name = file_path
        .file_name()
        .ok_or(file::Error::Other("unwrap of file_name failed".to_string()))?;
    let file_path = config.output_directory.join(PathBuf::from(file_name));

    debug!("storing at \"{}\"", file_path.display());

    if file_path.exists() {
        return Err(file::Error::Other(format!(
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
