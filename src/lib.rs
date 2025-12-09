use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, LevelFilter, TermLogger, TerminalMode, WriteLogger,
};
use std::fs::File;
use std::path::Path;

use std::path::PathBuf;

pub mod aux;
pub mod protocol;
pub mod receive;
pub mod semaphore;
pub mod send;

// Allow unsafe code to call libc function setsockopt.
#[allow(unsafe_code)]
pub mod sock_utils;

// Allow unsafe code to initialize C structs and call
// libc functions recv_mmsg and send_mmsg.
#[allow(unsafe_code)]
pub mod udp;

pub fn init_logger(log_path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let level_filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|rust_log| rust_log.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info);

    let config = ConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Right)
        .set_target_level(LevelFilter::Off)
        .set_thread_level(LevelFilter::Info)
        .set_thread_mode(simplelog::ThreadLogMode::Names)
        .set_time_format_rfc2822()
        .build();

    let mut loggers: Vec<Box<dyn simplelog::SharedLogger>> = Vec::new();

    loggers.push(TermLogger::new(
        level_filter,
        config.clone(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    ));

    if let Some(ref path) = log_path {
        match File::create(Path::new(&path)) {
            Ok(file) => {
                loggers.push(WriteLogger::new(level_filter, config, file));
                CombinedLogger::init(loggers)?;
            }
            Err(e) => {
                CombinedLogger::init(loggers)?;
                log::error!("Failed to create file at {:?}: {:?}", log_path, e);
            }
        }
    }

    Ok(())
}
