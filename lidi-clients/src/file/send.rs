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
#[cfg(feature = "inotify")]
use std::{io, thread};

#[cfg(feature = "inotify")]
fn notifier_thread(
    dir: &path::Path,
    to_send: &crossbeam_channel::Sender<path::PathBuf>,
) -> Result<(), file::Error> {
    let mut inotify = inotify::Inotify::init()?;

    inotify.watches().add(
        dir,
        inotify::WatchMask::CLOSE_WRITE | inotify::WatchMask::MOVED_TO,
    )?;

    let mut buffer = [0u8; 4096];

    loop {
        let events = inotify.read_events_blocking(&mut buffer)?;
        for event in events {
            if let Some(name) = event.name {
                let path = dir.join(name);
                to_send.send(path).map_err(|e| {
                    file::Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))
                })?;
            }
        }
    }
}

#[cfg(feature = "inotify")]
fn send_file_thread(
    config: &file::Config<crate::DiodeSend>,
    for_send: &crossbeam_channel::Receiver<path::PathBuf>,
) {
    let mut count = 0;

    loop {
        use std::ffi::OsStr;

        if config.max_files != 0 && count >= config.max_files {
            break;
        }

        let Ok(path) = for_send.recv() else {
            break;
        };

        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            log::error!("not a file {:?}", path.display());
            continue;
        };

        if let Some(ignore) = config.ignore.as_ref()
            && ignore.is_match(file_name)
        {
            log::debug!("ignoring {:?}", path.display());
            continue;
        }

        let Ok(path) = path
            .into_os_string()
            .into_string()
            .inspect_err(|e| log::error!("unsupported file name {}", e.display()))
        else {
            continue;
        };

        match send_file(config, &path) {
            Ok(total) => {
                log::info!("file {path:?} sent, {total} bytes sent");
            }
            Err(e) => {
                log::error!("failed to send file {path}: {e}");
            }
        }

        count += 1;
    }
}

pub fn send_dir(config: &file::Config<crate::DiodeSend>, path: &String) -> Result<(), file::Error> {
    let dir = path::PathBuf::from(path);

    if !dir.is_dir() {
        return Err(file::Error::Other(format!("{path:?} is not a directory")));
    }

    let (to_send, for_send) = crossbeam_channel::unbounded();

    thread::scope(|scope| {
        thread::Builder::new().spawn_scoped(scope, || send_file_thread(config, &for_send))?;

        #[cfg(feature = "inotify")]
        thread::Builder::new().spawn_scoped(scope, || {
            if let Err(e) = notifier_thread(&dir, &to_send) {
                log::error!("{e}");
            }
        })?;

        for file in dir
            .read_dir()?
            .filter_map(|entry| {
                entry
                    .inspect_err(|e| log::error!("failed to read entry: {e}"))
                    .ok()
                    .map(|entry| entry.path())
            })
            .filter(|path| path.is_file())
        {
            if to_send.send(file).is_err() {
                break;
            }
        }

        Ok(())
    })
}

/// # Errors
///
/// Will return `Err` if `send_file` function
/// returns an `Err`.
pub fn send_files(
    config: &file::Config<crate::DiodeSend>,
    paths: &[String],
) -> Result<(), file::Error> {
    for path in paths {
        let total = send_file(config, path)?;
        log::info!("file {path:?} sent, {total} bytes sent");
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
    path: &String,
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
                send_file_aux(config, diode, &path::PathBuf::from(path))
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
                send_file_aux(config, diode, &path::PathBuf::from(path))
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
                send_file_aux(config, diode, &path::PathBuf::from(path))
            }
        }
    }
}

fn send_file_aux<D>(
    config: &file::Config<crate::DiodeSend>,
    mut diode: D,
    file_path: &path::PathBuf,
) -> Result<usize, file::Error>
where
    D: Read + Write,
{
    log::debug!("opening file {}", file_path.display());

    if !file_path.is_file() {
        return Err(file::Error::Other(String::from("not a file")));
    }

    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(file_path)?;

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

    log::info!(
        "sending file {} ({} bytes)",
        file_path.display(),
        header.file_length
    );

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
