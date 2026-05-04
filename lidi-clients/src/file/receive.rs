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
    path, thread,
};

/// # Errors
///
/// Will return `Err` if `output_dir` is not a directory.
pub fn receive_files(
    config: &file::Config<crate::DiodeReceive>,
    output_dir: &path::Path,
) -> Result<(), file::Error> {
    if !output_dir.is_dir() {
        return Err(file::Error::Other(String::from(
            "output_directory is not a directory",
        )));
    }

    thread::scope(|scope| -> Result<(), file::Error> {
        #[cfg(feature = "tcp")]
        if let Some(from_tcp) = &config.diode.from_tcp {
            let server = net::TcpListener::bind(from_tcp)?;
            thread::Builder::new().spawn_scoped(scope, move || {
                receive_tcp_loop(config, output_dir, scope, &server)
            })?;
        }

        #[cfg(feature = "tls")]
        if let Some(from_tls) = &config.diode.from_tls {
            let server = tls::TcpListener::bind(&config.tls, from_tls)?;
            thread::Builder::new().spawn_scoped(scope, move || {
                receive_tls_loop(config, output_dir, scope, &server)
            })?;
        }

        #[cfg(feature = "unix")]
        if let Some(from_unix) = &config.diode.from_unix {
            if from_unix.exists() {
                return Err(file::Error::Other(format!(
                    "Unix socket path '{}' already exists",
                    from_unix.display()
                )));
            }

            let server = unix::net::UnixListener::bind(from_unix)?;
            thread::Builder::new().spawn_scoped(scope, move || {
                receive_unix_loop(config, output_dir, scope, &server)
            })?;
        }

        Ok(())
    })
}

#[cfg(feature = "tcp")]
fn receive_tcp_loop<'a>(
    config: &'a file::Config<crate::DiodeReceive>,
    output_dir: &'a path::Path,
    scope: &'a thread::Scope<'a, '_>,
    server: &net::TcpListener,
) -> Result<(), file::Error> {
    let mut count = 0;

    loop {
        if config.max_files != 0 && count >= config.max_files {
            return Ok(());
        }
        count += 1;
        let (client, client_addr) = server.accept()?;
        log::debug!("new TCP client ({client_addr}) connected");
        scope.spawn(|| match receive_file(config, client, output_dir) {
            Ok(total) => log::info!("file received, {total} bytes received"),
            Err(e) => log::error!("failed to receive file: {e}"),
        });
    }
}

#[cfg(feature = "tls")]
fn receive_tls_loop<'a>(
    config: &'a file::Config<crate::DiodeReceive>,
    output_dir: &'a path::Path,
    scope: &'a thread::Scope<'a, '_>,
    server: &tls::TcpListener,
) -> Result<(), file::Error> {
    let mut count = 0;

    loop {
        if config.max_files != 0 && count >= config.max_files {
            return Ok(());
        }
        count += 1;
        let (client, client_addr) = server.accept()??;
        log::info!("new TLS client ({client_addr}) connected");
        scope.spawn(|| match receive_file(config, client, output_dir) {
            Ok(total) => log::info!("file received, {total} bytes received"),
            Err(e) => log::error!("failed to receive file: {e}"),
        });
    }
}

#[cfg(feature = "unix")]
fn receive_unix_loop<'a>(
    config: &'a file::Config<crate::DiodeReceive>,
    output_dir: &'a path::Path,
    scope: &'a thread::Scope<'a, '_>,
    server: &unix::net::UnixListener,
) -> Result<(), file::Error> {
    let mut count = 0;

    loop {
        if config.max_files != 0 && count >= config.max_files {
            return Ok(());
        }
        count += 1;
        let (client, client_addr) = server.accept()?;
        log::info!(
            "new Unix client ({}) connected",
            client_addr
                .as_pathname()
                .map_or_else(|| String::from("unknown"), |p| p.display().to_string())
        );
        scope.spawn(|| match receive_file(config, client, output_dir) {
            Ok(total) => log::info!("file received, {total} bytes received"),
            Err(e) => log::error!("failed to receive file: {e}"),
        });
    }
}

fn receive_file<D>(
    config: &file::Config<crate::DiodeReceive>,
    mut diode: D,
    output_dir: &path::Path,
) -> Result<usize, file::Error>
where
    D: Read + Write,
{
    let header = file::protocol::Header::deserialize_from(&mut diode)?;

    let file_path = path::PathBuf::from_iter(&header.file_path);
    let file_path = output_dir.join(file_path);

    log::info!(
        "receiving file {} ({} bytes)",
        file_path.display(),
        header.file_length
    );

    if !config.overwrite && file_path.exists() {
        return Err(file::Error::Other(format!(
            "file {} already exists",
            file_path.display()
        )));
    }

    if let Some(parent) = file_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&file_path)?;

    log::debug!("setting mode to {}", header.mode);
    file.set_permissions(fs::Permissions::from_mode(header.mode))?;

    let mut buffer = vec![0; config.buffer_size];
    let mut cursor = 0;
    let mut remaining = usize::try_from(header.file_length)?;

    #[cfg(feature = "hash")]
    let mut hasher = if config.hash {
        Some(hash::StreamHasher::default())
    } else {
        None
    };

    loop {
        let end = if remaining >= (config.buffer_size - cursor) {
            config.buffer_size
        } else {
            cursor + remaining
        };
        match diode.read(&mut buffer[cursor..end])? {
            0 => {
                if 0 < cursor {
                    #[cfg(feature = "hash")]
                    if let Some(hasher) = hasher.as_mut() {
                        hasher.update(&buffer[..cursor]);
                    }
                    file.write_all(&buffer[..cursor])?;
                }

                file.flush()?;

                let received = usize::try_from(header.file_length)? - remaining;

                if remaining != 0 {
                    log::debug!("expected file size = {}", header.file_length);
                    log::debug!("received file size = {received}");
                    return Err(file::Error::Diode(file::protocol::Error::InvalidFileSize(
                        usize::try_from(header.file_length)?,
                        received,
                    )));
                }

                #[cfg(feature = "hash")]
                if let Some(hasher) = hasher.as_mut() {
                    let footer = file::protocol::Footer::deserialize_from(&mut diode)?;
                    let hash = hasher.finalize();
                    log::debug!("expected hash = {}", footer.hash);
                    log::debug!("computed hash = {hash}");
                    if footer.hash != hash {
                        return Err(file::Error::Diode(file::protocol::Error::InvalidHash(
                            hash,
                            footer.hash,
                        )));
                    }
                }

                return Ok(received);
            }
            nread => {
                remaining -= nread;
                if (cursor + nread) < config.buffer_size {
                    cursor += nread;
                    continue;
                }
                #[cfg(feature = "hash")]
                if let Some(hasher) = hasher.as_mut() {
                    hasher.update(&buffer);
                }
                file.write_all(&buffer)?;
                cursor = 0;
            }
        }
    }
}
