use std::{fs, path, sync::Arc};

pub mod aux;
pub mod http;
pub mod logring;
pub mod protocol;
pub mod receive;
pub mod send;
pub mod stats;
// Allow unsafe code to call libc function setsockopt.
#[allow(unsafe_code)]
mod sock_utils;
// Allow unsafe code to initialize C structs and call
// libc functions recv_mmsg and send_mmsg.
#[allow(unsafe_code)]
mod udp;

/// # Errors
///
/// Will return `Err` if `file` cannot be opened
/// or logger cannot be set (Term or file mode).
pub fn init_logger(
    level_filter: log::LevelFilter,
    file: Option<path::PathBuf>,
    stderr_only: bool,
) -> Result<(), String> {
    init_logger_with_ring(level_filter, file, stderr_only, false).map(|_| ())
}

/// Logger initializer with optional in-memory ring-buffer tee.
///
/// When `with_ring` is true, installs a tee logger that also pushes records
/// into an in-memory [`logring::LogRing`] returned to the caller so the
/// observability HTTP server can serve recent log lines. Otherwise behaves
/// identically to [`init_logger`].
///
/// # Errors
///
/// Will return `Err` if `file` cannot be opened
/// or logger cannot be set (Term or file mode).
pub fn init_logger_with_ring(
    level_filter: log::LevelFilter,
    file: Option<path::PathBuf>,
    stderr_only: bool,
    with_ring: bool,
) -> Result<Option<Arc<logring::LogRing>>, String> {
    let terminal_mode = if stderr_only {
        simplelog::TerminalMode::Stderr
    } else {
        simplelog::TerminalMode::Mixed
    };

    let config = simplelog::ConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Right)
        .set_target_level(simplelog::LevelFilter::Off)
        .set_thread_level(level_filter)
        .set_thread_mode(simplelog::ThreadLogMode::Names)
        .set_time_format_rfc2822()
        .set_time_offset_to_local()
        .unwrap_or_else(|e| e)
        .build();

    if with_ring {
        let inner: Box<dyn log::Log> = match file {
            Some(file) => {
                let fh = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .truncate(false)
                    .read(false)
                    .open(file)
                    .map_err(|e| e.to_string())?;
                simplelog::WriteLogger::new(level_filter, config, fh)
            }
            None => simplelog::TermLogger::new(
                level_filter,
                config,
                terminal_mode,
                simplelog::ColorChoice::Auto,
            ),
        };
        let ring = Arc::new(logring::LogRing::new(1024));
        let tee = logring::TeeLogger::new(inner, level_filter, Arc::clone(&ring));
        log::set_boxed_logger(Box::new(tee)).map_err(|e| e.to_string())?;
        log::set_max_level(level_filter);
        Ok(Some(ring))
    } else {
        match file {
            Some(file) => fs::OpenOptions::new()
                .create(true)
                .append(true)
                .truncate(false)
                .read(false)
                .open(file)
                .map_err(|e| e.to_string())
                .and_then(|file| {
                    simplelog::WriteLogger::init(level_filter, config, file)
                        .map_err(|e| e.to_string())
                })
                .map(|()| None),
            None => simplelog::TermLogger::init(
                level_filter,
                config,
                terminal_mode,
                simplelog::ColorChoice::Auto,
            )
            .map_err(|e| e.to_string())
            .map(|()| None),
        }
    }
}
