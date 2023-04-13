use crate::file;
use std::{
    fs,
    io::{Read, Write},
    net,
    os::unix::{self, fs::PermissionsExt},
    path, thread,
};

pub fn receive_files(
    config: &file::Config<file::DiodeReceive>,
    output_dir: &path::Path,
) -> Result<(), file::Error> {
    if !output_dir.is_dir() {
        return Err(file::Error::Other(
            "output_directory is not a directory".to_string(),
        ));
    }

    thread::scope(|scope| -> Result<(), file::Error> {
        if let Some(from_unix) = &config.diode.from_unix {
            if from_unix.exists() {
                return Err(file::Error::Other(format!(
                    "Unix socket path '{}' already exists",
                    from_unix.display()
                )));
            }

            let server = unix::net::UnixListener::bind(from_unix)?;
            thread::Builder::new().spawn_scoped(scope, || {
                receive_unix_loop(config, output_dir, scope, server)
            })?;
        }

        if let Some(from_tcp) = &config.diode.from_tcp {
            let server = net::TcpListener::bind(from_tcp)?;
            thread::Builder::new().spawn_scoped(scope, || {
                receive_tcp_loop(config, output_dir, scope, server)
            })?;
        }

        Ok(())
    })
}

fn receive_tcp_loop<'a>(
    config: &'a file::Config<file::DiodeReceive>,
    output_dir: &'a path::Path,
    scope: &'a thread::Scope<'a, '_>,
    server: net::TcpListener,
) -> Result<(), file::Error> {
    loop {
        let (client, client_addr) = server.accept()?;
        log::info!("new Unix client ({client_addr}) connected");
        scope.spawn(|| match receive_file(config, client, output_dir) {
            Ok(total) => log::info!("file received, {total} bytes received"),
            Err(e) => log::error!("failed to receive file: {e}"),
        });
    }
}

fn receive_unix_loop<'a>(
    config: &'a file::Config<file::DiodeReceive>,
    output_dir: &'a path::Path,
    scope: &'a thread::Scope<'a, '_>,
    server: unix::net::UnixListener,
) -> Result<(), file::Error> {
    loop {
        let (client, client_addr) = server.accept()?;
        log::info!(
            "new Unix client ({}) connected",
            client_addr
                .as_pathname()
                .map_or("unknown".to_string(), |p| p.display().to_string())
        );
        scope.spawn(|| match receive_file(config, client, output_dir) {
            Ok(total) => log::info!("file received, {total} bytes received"),
            Err(e) => log::error!("failed to receive file: {e}"),
        });
    }
}

fn receive_file<D>(
    config: &file::Config<file::DiodeReceive>,
    mut diode: D,
    output_dir: &path::Path,
) -> Result<usize, file::Error>
where
    D: Read + Write,
{
    let header = file::protocol::Header::deserialize_from(&mut diode)?;

    log::debug!("receiving file \"{}\"", header.file_name);

    let file_path = path::PathBuf::from(header.file_name);
    let file_name = file_path
        .file_name()
        .ok_or(file::Error::Other("unwrap of file_name failed".to_string()))?;
    let file_path = output_dir.join(path::PathBuf::from(file_name));

    log::debug!("storing at \"{}\"", file_path.display());

    if file_path.exists() {
        return Err(file::Error::Other(format!(
            "file \"{}\" already exists",
            file_path.display()
        )));
    }

    let mut file = fs::OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .open(&file_path)?;

    log::debug!("setting mode to {}", header.mode);
    file.set_permissions(fs::Permissions::from_mode(header.mode))?;

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
