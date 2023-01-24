use crate::file::{protocol, Config, Error};
use log::{debug, info};
use std::{
    fs::{OpenOptions, Permissions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    thread,
};

pub fn receive_files(config: Config, output_dir: PathBuf) -> Result<(), Error> {
    if !output_dir.is_dir() {
        return Err(Error::Other(
            "output_directory is not a directory".to_string(),
        ));
    }

    let server = TcpListener::bind(config.socket_addr)?;

    thread::scope(|scope| -> Result<(), Error> {
        for incoming in server.incoming() {
            let client = incoming?;
            scope.spawn(|| -> Result<(), Error> {
                let total = receive_file(&config, client, &output_dir)?;
                info!("file received, {total} bytes received");
                Ok(())
            });
        }
        Ok(())
    })?;

    Ok(())
}

pub fn receive_file(
    config: &Config,
    mut diode: TcpStream,
    output_dir: &Path,
) -> Result<usize, Error> {
    info!("new client connected");

    diode.shutdown(std::net::Shutdown::Write)?;

    let header = protocol::Header::deserialize_from(&mut diode)?;

    debug!("receiving file \"{}\"", header.file_name);

    let file_path = PathBuf::from(header.file_name);
    let file_name = file_path
        .file_name()
        .ok_or(Error::Other("unwrap of file_name failed".to_string()))?;
    let file_path = output_dir.join(PathBuf::from(file_name));

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
