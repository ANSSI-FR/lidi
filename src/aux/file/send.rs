use fasthash::HasherExt;

use crate::aux::{self, file};
use std::{
    fs,
    hash::Hash,
    io::{Read, Write},
    net,
    os::unix::{self, fs::PermissionsExt},
    path,
};

pub fn send_files(
    config: &file::Config<aux::DiodeSend>,
    files: &[String],
) -> Result<(), file::Error> {
    for file in files {
        let total = send_file(config, file)?;
        log::info!("file send, {total} bytes sent");
    }
    Ok(())
}

pub fn send_file(
    config: &file::Config<aux::DiodeSend>,
    file_path: &String,
) -> Result<usize, file::Error> {
    log::debug!("connecting to {}", config.diode);

    match &config.diode {
        aux::DiodeSend::Tcp(socket_addr) => {
            let diode = net::TcpStream::connect(socket_addr)?;
            send_file_aux(config, diode, file_path)
        }
        aux::DiodeSend::Unix(path) => {
            let diode = unix::net::UnixStream::connect(path)?;
            send_file_aux(config, diode, file_path)
        }
    }
}

fn send_file_aux<D>(
    config: &file::Config<aux::DiodeSend>,
    mut diode: D,
    file_path: &String,
) -> Result<usize, file::Error>
where
    D: Read + Write,
{
    log::debug!("opening file {file_path:?}");

    let file_path = path::PathBuf::from(file_path);

    if !file_path.is_file() {
        return Err(file::Error::Other("not a file".to_string()));
    }

    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(&file_path)?;

    let file_name = file_path
        .file_name()
        .ok_or(file::Error::Other("unwrap of file_name failed".to_string()))?
        .to_os_string()
        .into_string()
        .map_err(|_| file::Error::Other("conversion from OsString to String failed".to_string()))?;

    log::debug!("file name is {file_name:?}");

    let metadata = file.metadata()?;
    let permissions = metadata.permissions();

    let header = file::protocol::Header {
        file_name,
        mode: permissions.mode(),
        file_length: metadata.len(),
    };

    header.serialize_to(&mut diode)?;

    let mut buffer = vec![0; config.buffer_size];
    let mut cursor = 0;
    let mut total = 0;

    let mut hasher = fasthash::Murmur3HasherExt::default();

    loop {
        match file.read(&mut buffer[cursor..])? {
            0 => {
                if 0 < cursor {
                    total += cursor;
                    if config.hash {
                        buffer[..cursor].hash(&mut hasher);
                    }
                    diode.write_all(&buffer[..cursor])?;
                }

                let footer = file::protocol::Footer {
                    hash: if config.hash { hasher.finish_ext() } else { 0 },
                };

                footer.serialize_to(&mut diode)?;

                diode.flush()?;
                return Ok(total);
            }
            nread => {
                if (cursor + nread) < config.buffer_size {
                    cursor += nread;
                    continue;
                }
                total += config.buffer_size;
                if config.hash {
                    buffer.hash(&mut hasher);
                }
                diode.write_all(&buffer)?;
                cursor = 0;
            }
        }
    }
}
