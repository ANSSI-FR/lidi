#[cfg(feature = "tls")]
use crate::tls;
use crate::{file, hash};
#[cfg(feature = "tcp")]
use std::net;
#[cfg(feature = "unix")]
use std::os::unix;
#[cfg(feature = "inotify")]
use std::{collections, io, thread};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path,
};

#[cfg(feature = "inotify")]
fn notifier_thread(
    dir: path::PathBuf,
    recursive: bool,
    to_send: &crossbeam_channel::Sender<Option<path::PathBuf>>,
) -> Result<(), file::Error> {
    let mut inotify = inotify::Inotify::init()?;

    let mut watch_mask: inotify::WatchMask =
        inotify::WatchMask::CLOSE_WRITE | inotify::WatchMask::MOVED_TO;
    if recursive {
        watch_mask |= inotify::WatchMask::CREATE;
    }

    let mut descriptors = collections::HashMap::new();

    let mut todo = collections::HashSet::new();
    todo.insert(dir);

    loop {
        if todo.is_empty() {
            break;
        }

        let mut next = collections::HashSet::new();

        for dir in todo {
            log::debug!("watch {}", dir.display());
            let wd = inotify.watches().add(dir.as_path(), watch_mask)?;
            descriptors.insert(wd.get_watch_descriptor_id(), dir.clone());

            if recursive {
                for dir in dir
                    .read_dir()?
                    .filter_map(|entry| {
                        entry
                            .inspect_err(|e| log::error!("failed to read entry: {e}"))
                            .ok()
                            .map(|entry| entry.path())
                    })
                    .filter(|path| path.is_dir())
                {
                    next.insert(dir);
                }
            }
        }

        todo = next;
    }

    let mut buffer = [0u8; 4096];

    loop {
        let events = inotify.read_events_blocking(&mut buffer)?;
        for event in events {
            if let Some(name) = event.name {
                match descriptors.get(&event.wd.get_watch_descriptor_id()) {
                    None => {
                        log::warn!("no descriptor found for event on {}", name.display());
                    }
                    Some(dir) => {
                        let path = dir.join(name);
                        if path.is_dir() && event.mask.contains(inotify::EventMask::CREATE) {
                            log::debug!("watch created dir {}", path.display());
                            let wd = inotify.watches().add(path.clone(), watch_mask)?;
                            descriptors.insert(wd.get_watch_descriptor_id(), path);
                        } else if path.is_file()
                            && (event.mask.contains(inotify::EventMask::CLOSE_WRITE)
                                || event.mask.contains(inotify::EventMask::MOVED_TO))
                        {
                            log::debug!("watch new file {}", path.display());
                            to_send.send(Some(path)).map_err(|e| {
                                file::Error::Io(io::Error::new(
                                    io::ErrorKind::BrokenPipe,
                                    e.to_string(),
                                ))
                            })?;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "inotify")]
fn send_file_thread(
    config: &file::Config<crate::DiodeSend>,
    for_send: &crossbeam_channel::Receiver<Option<path::PathBuf>>,
    base_dir: Option<&path::PathBuf>,
) {
    let mut count = 0;

    loop {
        if config.max_files != 0 && count >= config.max_files {
            break;
        }

        let Ok(path) = for_send.recv() else {
            break;
        };

        let Some(path) = path else {
            return;
        };

        let Some(file_path) = path.as_os_str().to_str() else {
            log::error!("not a file {:?}", path.display());
            continue;
        };

        if let Some(ignore) = config.ignore.as_ref()
            && ignore.is_match(file_path)
        {
            log::debug!("ignoring {:?}", path.display());
            continue;
        }

        match send_file(config, path.as_path(), base_dir) {
            Ok(total) => {
                log::info!("file {} sent, {total} bytes sent", path.display());
            }
            Err(e) => {
                log::error!("failed to send file {}: {e}", path.display());
            }
        }

        count += 1;
    }
}

pub fn send_dir(
    config: &file::Config<crate::DiodeSend>,
    path: &path::Path,
) -> Result<(), file::Error> {
    let dir = path::PathBuf::from(path);

    if !dir.is_dir() {
        return Err(file::Error::Other(format!(
            "{} is not a directory",
            path.display()
        )));
    }

    let (to_send, for_send) = crossbeam_channel::unbounded();

    thread::scope(|scope| {
        let ldir = dir.clone();
        thread::Builder::new().spawn_scoped(scope, move || {
            send_file_thread(config, &for_send, Some(&ldir));
        })?;

        if config.watch {
            #[cfg(not(feature = "inotify"))]
            log::warn!("cannot watch directory because inotify was not enabled at compilation");
            #[cfg(feature = "inotify")]
            {
                let dir = dir.clone();
                thread::Builder::new().spawn_scoped(scope, || {
                    if let Err(e) = notifier_thread(dir, config.recursive, &to_send) {
                        log::error!("{e}");
                    }
                })?;
            }
        }

        let mut todo = collections::HashSet::new();
        todo.insert(dir);

        'outer: loop {
            if todo.is_empty() {
                break;
            }

            let mut next = collections::HashSet::new();

            for dir in todo {
                for entry in dir
                    .read_dir()?
                    .filter_map(|entry| {
                        entry
                            .inspect_err(|e| log::error!("failed to read entry: {e}"))
                            .ok()
                            .map(|entry| entry.path())
                    })
                    .filter(|entry| config.recursive || entry.is_file())
                {
                    if entry.is_dir() {
                        next.insert(entry);
                    } else if entry.is_file() && to_send.send(Some(entry)).is_err() {
                        break 'outer;
                    }
                }
            }

            todo = next;
        }

        if !config.watch {
            to_send
                .send(None)
                .map_err(|_| file::Error::Other(String::from("failed to stop sender thread")))?;
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
    paths: Vec<path::PathBuf>,
    base_dir: Option<&path::PathBuf>,
) -> Result<(), file::Error> {
    for path in paths {
        let total = send_file(config, path.as_path(), base_dir)?;
        log::info!("file {} sent, {total} bytes sent", path.display());
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
    path: &path::Path,
    base_dir: Option<&path::PathBuf>,
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
                send_file_aux(config, diode, path, base_dir)
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
                send_file_aux(config, diode, path, base_dir)
            }
        }
        crate::DiodeSend::Unix(spath) => {
            #[cfg(not(feature = "unix"))]
            {
                let _ = spath;
                log::error!("Unix was not enable at compilation");
                Ok(0)
            }
            #[cfg(feature = "unix")]
            {
                let diode = unix::net::UnixStream::connect(spath)?;
                send_file_aux(config, diode, path, base_dir)
            }
        }
    }
}

fn send_file_aux<D>(
    config: &file::Config<crate::DiodeSend>,
    mut diode: D,
    file_path: &path::Path,
    base_dir: Option<&path::PathBuf>,
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

    let metadata = file.metadata()?;
    let permissions = metadata.permissions();

    let file_path = if let Some(base_dir) = base_dir {
        let mut paths = vec![];
        let file_path = file_path.strip_prefix(base_dir).map_err(|_| {
            file::Error::Other(format!(
                "file {} is not in {}",
                file_path.display(),
                base_dir.display()
            ))
        })?;
        for path in file_path.components() {
            paths.push(path.as_os_str().to_os_string().into_string().map_err(|_| {
                file::Error::Other(String::from("conversion from OsString to String failed"))
            })?);
        }
        paths
    } else {
        vec![
            file_path
                .file_name()
                .ok_or_else(|| file::Error::Other(String::from("unwrap of file_name failed")))?
                .to_os_string()
                .into_string()
                .map_err(|_| {
                    file::Error::Other(String::from("conversion from OsString to String failed"))
                })?,
        ]
    };

    let header = file::protocol::Header {
        file_path,
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
        "sending file {:?} ({} bytes)",
        header.file_path,
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
