use crate::file::{self, EntryType};
#[cfg(feature = "hash")]
use crate::hash;
#[cfg(feature = "tls")]
use crate::tls;
#[cfg(feature = "inotify")]
use std::io;
#[cfg(feature = "tcp")]
use std::net;
#[cfg(feature = "unix")]
use std::os::unix;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections, fs,
    io::{Read, Write},
    path, thread,
};

/// Returns the entry's Unix permission bits, or a fixed placeholder on platforms without a
/// POSIX permission model (e.g. Windows).
#[cfg(unix)]
fn entry_mode(metadata: &fs::Metadata) -> u32 {
    metadata.permissions().mode()
}
#[cfg(not(unix))]
fn entry_mode(_metadata: &fs::Metadata) -> u32 {
    0o644
}

#[cfg(not(feature = "inotify"))]
use std::time::Duration;

/// Delay between two directory scans when watching for new files without
/// inotify support.
#[cfg(not(feature = "inotify"))]
const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(PartialEq, Eq)]
enum DirBehaviour {
    Enter,
    Skip,
}

fn walk_dir<F>(dir: path::PathBuf, recursive: bool, mut f: F) -> Result<(), file::Error>
where
    F: FnMut(&path::Path, &fs::Metadata) -> Result<DirBehaviour, file::Error>,
{
    let mut todo = collections::VecDeque::new();
    todo.push_back(dir);

    while let Some(dir) = todo.pop_front() {
        for (path, metadata) in dir.read_dir()?.filter_map(|entry| {
            entry
                .and_then(|e| Ok((e.path(), e.metadata()?)))
                .inspect_err(|e| log::error!("failed to read entry: {e}"))
                .ok()
        }) {
            let behaviour = f(&path, &metadata)?;
            if recursive && metadata.file_type().is_dir() && behaviour == DirBehaviour::Enter {
                todo.push_back(path);
            }
        }
    }
    Ok(())
}

fn should_ignore(ignore: Option<&regex::Regex>, path: &path::Path) -> bool {
    if let Some(ignore) = ignore
        && let Some(file_name) = path.file_name().and_then(|name| name.to_str())
        && ignore.is_match(file_name)
    {
        true
    } else {
        false
    }
}

#[cfg(feature = "inotify")]
fn notifier_new_dir(
    dir: path::PathBuf,
    config: &file::Config<crate::DiodeSend>,
    inotify: &inotify::Inotify,
    watch_mask: inotify::WatchMask,
    descriptors: &mut collections::HashMap<i32, path::PathBuf>,
    to_send: &crossbeam_channel::Sender<Option<path::PathBuf>>,
    send_files: bool,
) -> Result<(), file::Error> {
    let mut process = |path: &path::Path, metadata: &fs::Metadata, add_watch: bool| {
        // avoid sending or entering a dir if its name matches the ignore config
        if should_ignore(config.ignore.as_ref(), path) {
            return Ok(DirBehaviour::Skip);
        }

        if add_watch && metadata.file_type().is_dir() {
            log::debug!("watch {}", path.display());
            let wd = inotify.watches().add(path, watch_mask)?;
            descriptors.insert(wd.get_watch_descriptor_id(), path.to_owned());
        } else if send_files && metadata.file_type().is_file() {
            to_send.send(Some(path.to_owned())).map_err(|e| {
                file::Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))
            })?;
        }
        Ok(DirBehaviour::Enter)
    };

    let metadata = fs::metadata(&dir)?;
    process(&dir, &metadata, true)?;

    walk_dir(dir, config.recursive, |path, metadata| {
        process(path, metadata, config.recursive)
    })?;
    Ok(())
}

#[cfg(feature = "inotify")]
fn notifier_thread(
    dir: path::PathBuf,
    config: &file::Config<crate::DiodeSend>,
    to_send: &crossbeam_channel::Sender<Option<path::PathBuf>>,
) -> Result<(), file::Error> {
    let mut inotify = inotify::Inotify::init()?;

    let watch_mask: inotify::WatchMask =
        inotify::WatchMask::CLOSE_WRITE | inotify::WatchMask::MOVED_TO | inotify::WatchMask::CREATE;

    let mut descriptors = collections::HashMap::new();

    notifier_new_dir(
        dir,
        config,
        &inotify,
        watch_mask,
        &mut descriptors,
        to_send,
        false,
    )?;

    // In static mode or in non-recursive mode, we must send directories
    // instead of watching them
    let should_send_dirs = config.static_watch || !config.recursive;

    let mut buffer = [0u8; 4096];

    loop {
        let events = inotify.read_events_blocking(&mut buffer)?;
        for event in events {
            let Some(name) = event.name else {
                continue;
            };
            let Some(dir) = descriptors.get(&event.wd.get_watch_descriptor_id()) else {
                log::warn!("no descriptor found for event on {}", name.display());
                continue;
            };
            let path = dir.join(name);
            let metadata = path.metadata()?;

            if let Some(ignore) = config.ignore.as_ref()
                && let Some(file_name) = name.to_str()
                && ignore.is_match(file_name)
            {
                log::debug!("ignoring {:?}", path.display());
                continue;
            }

            if (should_send_dirs && metadata.is_dir())
                || (metadata.is_file()
                    && event
                        .mask
                        .intersects(inotify::EventMask::CLOSE_WRITE | inotify::EventMask::MOVED_TO))
            {
                // send the file or the directory
                log::debug!("watch: new file or dir to send: {}", path.display());
                to_send.send(Some(path)).map_err(|e| {
                    file::Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))
                })?;
            } else if metadata.is_dir() {
                // watch the new directory
                log::debug!("watch: new dir to watch: {}", path.display());
                notifier_new_dir(
                    path,
                    config,
                    &inotify,
                    watch_mask,
                    &mut descriptors,
                    to_send,
                    true,
                )?;
            }
        }
    }
}

/// Without inotify, new files are detected by periodically re-scanning the
/// directory tree and comparing it to the previous scan.
// `dir` is taken by value to match the inotify variant of this function.
#[cfg(not(feature = "inotify"))]
#[allow(clippy::needless_pass_by_value)]
fn notifier_thread(
    dir: path::PathBuf,
    config: &file::Config<crate::DiodeSend>,
    to_send: &crossbeam_channel::Sender<Option<path::PathBuf>>,
) -> Result<(), file::Error> {
    // In static mode, we want to track directories. In non-recursive mode, we
    // want to send top-level directories, so we need to include them as well
    let include_dir = config.static_watch || !config.recursive;

    let mut seen = collections::HashSet::new();
    walk_dir(dir.clone(), config.recursive, |path, metadata| {
        if include_dir || metadata.file_type().is_file() {
            seen.insert(path.to_owned());
        }
        Ok(DirBehaviour::Enter)
    })?;

    loop {
        thread::sleep(POLL_INTERVAL);
        let mut new_seen = collections::HashSet::new();

        walk_dir(dir.clone(), config.recursive, |path, metadata| {
            // avoid sending or entering a dir if its name matches the ignore config
            if should_ignore(config.ignore.as_ref(), path) {
                return Ok(DirBehaviour::Skip);
            }

            if include_dir || metadata.file_type().is_file() {
                new_seen.insert(path.to_owned());
                // if we already saw the entry, skip it
                if seen.contains(path) {
                    return Ok(DirBehaviour::Enter);
                }

                if metadata.is_dir() && config.recursive {
                    // add all the hierarchy to the `seen` map
                    walk_dir(path.to_owned(), true, |p, _| {
                        new_seen.insert(p.to_owned());
                        Ok(DirBehaviour::Enter)
                    })?;
                }

                log::debug!("watch: new file or dir to send: {}", path.display());
                to_send.send(Some(path.to_owned())).map_err(|e| {
                    file::Error::Other(format!("failed to send to send_file_thread: {e}"))
                })?;

                // Do not enter in the directory: sending a dir already sends its content
                return Ok(DirBehaviour::Skip);
            }
            Ok(DirBehaviour::Enter)
        })?;

        seen = new_seen;
    }
}

fn send_file_thread(
    config: &file::Config<crate::DiodeSend>,
    for_send: &crossbeam_channel::Receiver<Option<path::PathBuf>>,
    base_dir: Option<&path::Path>,
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

        if let Some(ignore) = config.ignore.as_ref()
            && let Some(file_name) = path.file_name().and_then(|s| s.to_str())
            && ignore.is_match(file_name)
        {
            log::debug!("ignoring {:?}", path.display());
            continue;
        }

        if let Err(e) = send_entry(config, path.as_path(), base_dir) {
            log::error!("failed to send file {}: {e}", path.display());
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
            let dir = dir.clone();
            thread::Builder::new().spawn_scoped(scope, || {
                if let Err(e) = notifier_thread(dir, config, &to_send) {
                    log::error!("{e}");
                }
            })?;
        }

        let mut closed = false;
        walk_dir(dir, config.recursive, |path, metadata| {
            if closed {
                return Ok(DirBehaviour::Skip);
            }
            if should_ignore(config.ignore.as_ref(), path) {
                return Ok(DirBehaviour::Skip);
            }
            // if not --recursive, we send the directories that we see at the top level
            if (metadata.is_file() || !config.recursive)
                && to_send.send(Some(path.to_owned())).is_err()
            {
                closed = true;
            }
            Ok(DirBehaviour::Enter)
        })?;

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
    base_dir: Option<&path::Path>,
) -> Result<(), file::Error> {
    for path in paths {
        send_entry(config, path.as_path(), base_dir)?;
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
fn connect_to_diode(
    config: &file::Config<crate::DiodeSend>,
) -> Result<Box<dyn Write>, file::Error> {
    log::debug!("connecting to {}", config.diode);

    Ok(match &config.diode {
        crate::DiodeSend::Tcp(socket_addr) => {
            #[cfg(not(feature = "tcp"))]
            {
                let _ = socket_addr;
                log::error!("TCP was not enable at compilation");
                return Err(file::Error::Other(
                    "TCP was not enable at compilation".to_owned(),
                ));
            }
            #[cfg(feature = "tcp")]
            {
                Box::new(net::TcpStream::connect(socket_addr)?)
            }
        }
        crate::DiodeSend::Tls(socket_addr) => {
            #[cfg(not(feature = "tls"))]
            {
                let _ = socket_addr;
                log::error!("TLS was not enable at compilation");
                return Err(file::Error::Other(
                    "TLS was not enable at compilation".to_owned(),
                ));
            }
            #[cfg(feature = "tls")]
            {
                let context = tls::ClientContext::try_from(&config.tls)?;
                Box::new(tls::TcpStream::connect(&context, socket_addr)?)
            }
        }
        crate::DiodeSend::Unix(spath) => {
            #[cfg(not(feature = "unix"))]
            {
                let _ = spath;
                log::error!("Unix was not enable at compilation");
                return Err(file::Error::Other(
                    "Unix was not enable at compilation".to_owned(),
                ));
            }
            #[cfg(feature = "unix")]
            {
                Box::new(unix::net::UnixStream::connect(spath)?)
            }
        }
    })
}

pub fn send_entry(
    config: &file::Config<crate::DiodeSend>,
    path: &path::Path,
    base_dir: Option<&path::Path>,
) -> Result<usize, file::Error> {
    let diode = connect_to_diode(config)?;

    let metadata = fs::metadata(path)?;
    let (entry_type, size) = if metadata.is_dir() {
        (
            EntryType::Directory,
            send_dir_entry(diode, config, path, base_dir, &metadata)?,
        )
    } else if metadata.is_file() {
        (
            EntryType::File,
            send_file_entry(diode, config, path, base_dir, &metadata)?,
        )
    } else {
        return Err(file::Error::Other("not a file or a dir".into()));
    };

    log::info!(
        "{entry_type} \"{}\" sent, {size} bytes sent",
        path.display()
    );

    Ok(size)
}

pub fn send_file_entry<D>(
    mut diode: D,
    config: &file::Config<crate::DiodeSend>,
    path: &path::Path,
    base_dir: Option<&path::Path>,
    metadata: &fs::Metadata,
) -> Result<usize, file::Error>
where
    D: Write,
{
    let res = send_file_aux(config, &mut diode, path, base_dir, metadata)?;

    if config.delete
        && let Err(e) = fs::remove_file(path)
    {
        log::error!("failed to delete file {}: {e}", path.display());
    }

    Ok(res)
}

pub fn send_dir_entry<D>(
    mut diode: D,
    config: &file::Config<crate::DiodeSend>,
    path: &path::Path,
    base_dir: Option<&path::Path>,
    metadata: &fs::Metadata,
) -> Result<usize, file::Error>
where
    D: Write,
{
    log::info!("sending directory \"{}\"", path.display());

    let start_msg = file::protocol::Message::StartDirTransfer(file::protocol::EntryInfo {
        path: path_to_vec(path, base_dir)?,
        mode: entry_mode(metadata),
    });
    start_msg.serialize_to(&mut diode)?;

    let mut total_size = 0;

    walk_dir(path.into(), true, |entry_path, metadata| {
        if metadata.file_type().is_dir() {
            log::info!("sending subdirectory \"{}\"", entry_path.display());
            let message = file::protocol::Message::DirEntry(file::protocol::EntryInfo {
                path: path_to_vec(entry_path, Some(path))?,
                mode: entry_mode(metadata),
            });
            message.serialize_to(&mut diode)?;
        } else if metadata.file_type().is_file() {
            total_size += send_file_aux(config, &mut diode, entry_path, Some(path), metadata)?;
        } else {
            log::error!("ignoring {}: not a file or a dir", entry_path.display());
        }
        Ok(DirBehaviour::Enter)
    })?;

    let end_msg = file::protocol::Message::EndDirTransfer;
    end_msg.serialize_to(&mut diode)?;

    if config.delete
        && let Err(e) = fs::remove_dir_all(path)
    {
        log::error!("failed to delete directory {}: {e}", path.display());
    }

    Ok(total_size)
}

fn send_file_aux<D>(
    config: &file::Config<crate::DiodeSend>,
    mut diode: &mut D,
    file_path: &path::Path,
    base_dir: Option<&path::Path>,
    metadata: &fs::Metadata,
) -> Result<usize, file::Error>
where
    D: Write,
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

    let message = file::protocol::Message::FileEntry(file::protocol::FileHeader {
        info: file::protocol::EntryInfo {
            path: path_to_vec(file_path, base_dir)?,
            mode: entry_mode(metadata),
        },
        file_length: metadata.len(),
    });

    message.serialize_to(&mut diode)?;

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
        "sending file \"{}\" ({} bytes)",
        file_path.display(),
        metadata.len()
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

fn path_to_vec(
    mut file_path: &path::Path,
    base_dir: Option<&path::Path>,
) -> Result<Vec<String>, file::Error> {
    Ok(if let Some(base_dir) = base_dir {
        let mut paths: Vec<String> = vec![];
        file_path = file_path.strip_prefix(base_dir).map_err(|_| {
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
    })
}
