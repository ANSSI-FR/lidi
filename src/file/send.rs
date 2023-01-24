use crate::file::{protocol, Config, Error};
use log::{debug, info};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::TcpStream,
    os::unix::prelude::PermissionsExt,
    path::PathBuf,
};

pub fn send_files(config: Config, files: Vec<String>) -> Result<(), Error> {
    for file in &files {
        let total = send_file(&config, file)?;
        info!("file send, {total} bytes sent");
    }
    Ok(())
}

pub fn send_file(config: &Config, file_path: &String) -> Result<usize, Error> {
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

    debug!("connecting to {}", config.socket_addr);

    let mut diode = TcpStream::connect(config.socket_addr)?;

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
