use fasthash::HasherExt;

use crate::file;
use std::{
    fs,
    hash::Hash,
    io::{Read, Write},
    net,
    os::unix::fs::PermissionsExt,
    path,
};

pub fn send_files(config: &file::Config, files: &[String]) -> Result<(), file::Error> {
    log::debug!("connecting to {}", config.diode);
    let mut diode = net::TcpStream::connect(config.diode)?;
    files.iter().enumerate().for_each(|(count, file)| {
        match send_file(config, &mut diode, file, count == files.len() - 1) {
            Ok(total) => log::info!("{file} sent, {total} bytes"),
            Err(e) => log::error!("Cannot send file {file}: {e}"),
        }
    });
    Ok(())
}

pub fn send_file(
    config: &file::Config,
    diode: &mut net::TcpStream,
    file_path: &str,
    stream_end: bool,
) -> Result<usize, file::Error> {
    log::debug!("opening file \"{}\"", file_path);

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

    log::debug!("file name is \"{file_name}\"");

    let metadata = file.metadata()?;
    let permissions = metadata.permissions();

    let header = file::protocol::Header {
        file_name,
        mode: permissions.mode(),
        file_length: metadata.len(),
    };

    header.serialize_to(diode)?;

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
                    stream_end,
                };

                footer.serialize_to(diode)?;

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
