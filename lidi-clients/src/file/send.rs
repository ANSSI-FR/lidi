#[cfg(feature = "tls")]
use crate::tls;
use crate::{file, hash};
#[cfg(feature = "tcp")]
use std::net;
#[cfg(feature = "unix")]
use std::os::unix;
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path,
};

/// # Errors
///
/// Will return `Err` if `send_file` function
/// returns an `Err`.
pub fn send_files(
    config: &file::Config<crate::DiodeSend>,
    files: &[String],
) -> Result<(), file::Error> {
    for file in files {
        let total = send_file(config, file)?;
        log::info!("file send, {total} bytes sent");
    }
    Ok(())
}

/// # Errors
///
/// Will return `Err` if:
/// - `net::TcpStream::connect(socket_addr)?`
///   or
/// - `unix::net::UnixStream::connect(path)?`
///   fails.
pub fn send_file(
    config: &file::Config<crate::DiodeSend>,
    file_path: &String,
) -> Result<usize, file::Error> {
    log::debug!("connecting to {}", config.diode);

    match &config.diode {
        crate::DiodeSend::Tcp(socket_addr) => {
            #[cfg(not(feature = "tcp"))]
            {
                let _ = socket_addr;
                log::error!("TCP was not enable at compilation");
                Ok(0)
            }
            #[cfg(feature = "tcp")]
            {
                let diode = net::TcpStream::connect(socket_addr)?;
                send_file_aux(config, diode, file_path)
            }
        }
        crate::DiodeSend::Tls(socket_addr) => {
            #[cfg(not(feature = "tls"))]
            {
                let _ = socket_addr;
                log::error!("TLS was not enable at compilation");
                Ok(0)
            }
            #[cfg(feature = "tls")]
            {
                let context = tls::ClientContext::try_from(&config.tls)?;
                let diode = tls::TcpStream::connect(&context, socket_addr)?;
                send_file_aux(config, diode, file_path)
            }
        }
        crate::DiodeSend::Unix(path) => {
            #[cfg(not(feature = "unix"))]
            {
                let _ = path;
                log::error!("Unix was not enable at compilation");
                Ok(0)
            }
            #[cfg(feature = "unix")]
            {
                let diode = unix::net::UnixStream::connect(path)?;
                send_file_aux(config, diode, file_path)
            }
        }
    }
}

fn send_file_aux<D>(
    config: &file::Config<crate::DiodeSend>,
    mut diode: D,
    file_path: &String,
) -> Result<usize, file::Error>
where
    D: Read + Write,
{
    log::debug!("opening file {file_path:?}");

    let file_path = path::PathBuf::from(file_path);

    if !file_path.is_file() {
        return Err(file::Error::Other(String::from("not a file")));
    }

    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(&file_path)?;

    let file_name = file_path
        .file_name()
        .ok_or_else(|| file::Error::Other(String::from("unwrap of file_name failed")))?
        .to_os_string()
        .into_string()
        .map_err(|_| {
            file::Error::Other(String::from("conversion from OsString to String failed"))
        })?;

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

    #[cfg(feature = "hash")]
    let mut hasher = if config.hash {
        Some(hash::StreamHasher::default())
    } else {
        None
    };

    loop {
        match file.read(&mut buffer[cursor..])? {
            0 => {
                if 0 < cursor {
                    total += cursor;
                    #[cfg(feature = "hash")]
                    if let Some(hasher) = hasher.as_mut() {
                        hasher.update(&buffer[..cursor]);
                    }
                    diode.write_all(&buffer[..cursor])?;
                }

                let footer = file::protocol::Footer {
                    #[cfg(feature = "hash")]
                    hash: hasher.as_mut().map_or(0, |hasher| hasher.finalize()),
                    #[cfg(not(feature = "hash"))]
                    hash: 0,
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
                #[cfg(feature = "hash")]
                if let Some(hasher) = hasher.as_mut() {
                    hasher.update(&buffer);
                }
                diode.write_all(&buffer)?;
                cursor = 0;
            }
        }
    }
}
