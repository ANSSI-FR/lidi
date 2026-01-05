use std::{fs, path};

pub mod aux;
pub mod protocol;
pub mod receive;
pub mod send;
// Allow unsafe code to call libc function setsockopt.
#[allow(unsafe_code)]
mod sock_utils;
// Allow unsafe code to initialize C structs and call
// libc functions recv_mmsg and send_mmsg.
#[allow(unsafe_code)]
mod udp;

pub fn init_logger(
    level_filter: log::LevelFilter,
    file: Option<path::PathBuf>,
    stderr_only: bool,
) -> Result<(), String> {
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

    match file {
        Some(file) => fs::OpenOptions::new()
            .create(true)
            .append(true)
            .truncate(false)
            .read(false)
            .open(file)
            .map_err(|e| e.to_string())
            .and_then(|file| {
                simplelog::WriteLogger::init(level_filter, config, file).map_err(|e| e.to_string())
            }),
        None => simplelog::TermLogger::init(
            level_filter,
            config,
            terminal_mode,
            simplelog::ColorChoice::Auto,
        )
        .map_err(|e| e.to_string()),
    }
}
