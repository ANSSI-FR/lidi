use fasthash::HasherExt;

use crate::file::{self, protocol};
use std::{
    fs,
    hash::Hash,
    io::{Read, Write},
    net,
    os::unix::fs::PermissionsExt,
    path,
};

pub fn receive_files(config: &file::Config, output_dir: &path::Path) -> Result<(), file::Error> {
    if !output_dir.is_dir() {
        return Err(file::Error::Other(
            "output_directory is not a directory".to_string(),
        ));
    }

    receive_tcp_loop(config, output_dir)?;

    Ok(())
}

fn receive_tcp_loop(config: &file::Config, output_dir: &path::Path) -> Result<(), file::Error> {
    loop {
        // quit loop in case of error to force reconnecting
        log::trace!("new tcp receive file");
        let server = net::TcpListener::bind(config.diode)?;
        receive_tcp_socket(config, output_dir, server)?;
    }
}

fn receive_tcp_socket(
    config: &file::Config,
    output_dir: &path::Path,
    server: net::TcpListener,
) -> Result<(), file::Error> {
    let (mut client, client_addr) = server.accept()?;
    log::debug!("new Unix client ({client_addr}) connected");
    drop(server);
    loop {
        match receive_file(config, &mut client, output_dir) {
            Ok((filename, total, stream_end)) => {
                log::info!("{filename} received, {total} bytes");
                if stream_end {
                    return Ok(());
                }
            }
            Err(e) => {
                log::error!("failed to receive file: {e}");
                return Err(e);
            }
        }
    }
}

fn finish_file(
    config: &file::Config,
    diode: &mut net::TcpStream,
    mut file: fs::File,
    header: file::protocol::Header,
    hasher: fasthash::Murmur3HasherExt,
) -> Result<(String, usize, bool), file::Error> {
    file.flush()?;

    log::trace!("parsing footer");
    let footer = file::protocol::Footer::deserialize_from(diode)?;

    if config.hash {
        let hash = hasher.finish_ext();
        log::debug!("expected hash = {}", footer.hash);
        log::debug!("computed hash = {hash}");
        if footer.hash != hash {
            return Err(file::Error::Diode(protocol::Error::InvalidHash(
                hash,
                footer.hash,
            )));
        }
    }
    Ok((
        header.file_name,
        header.file_length as usize,
        footer.stream_end,
    ))
}

fn receive_file(
    config: &file::Config,
    diode: &mut net::TcpStream,
    output_dir: &path::Path,
) -> Result<(String, usize, bool), file::Error> {
    log::trace!("parsing header");
    let header = file::protocol::Header::deserialize_from(diode)?;

    log::debug!("receiving file \"{}\"", header.file_name);
    log::debug!("file size = {}", header.file_length);

    let file_path = path::PathBuf::from(header.file_name.clone());
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
        .truncate(true)
        .open(&file_path)?;

    log::debug!("setting mode to {}", header.mode);
    file.set_permissions(fs::Permissions::from_mode(header.mode))?;

    let mut buffer = vec![0; config.buffer_size];
    let mut remaining = header.file_length as usize;

    let mut hasher = fasthash::Murmur3HasherExt::default();

    loop {
        let end = if remaining >= (config.buffer_size) {
            config.buffer_size
        } else {
            remaining
        };

        log::trace!("reading up to {end} bytes");

        match diode.read(&mut buffer[..end])? {
            0 => {
                if remaining != 0 {
                    let received = header.file_length as usize - remaining;
                    log::debug!("expected file size = {}", header.file_length);
                    log::debug!("received file size = {received}");
                    return Err(file::Error::Diode(protocol::Error::InvalidFileSize(
                        header.file_length as usize,
                        received,
                    )));
                }

                return finish_file(config, diode, file, header, hasher);
            }
            nread => {
                remaining -= nread;

                if config.hash {
                    buffer[..nread].hash(&mut hasher);
                }
                file.write_all(&buffer[..nread])?;

                if remaining == 0 {
                    return finish_file(config, diode, file, header, hasher);
                }
            }
        }
    }
}
