use std::str::FromStr;

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

pub fn init_logger() {
    let level_filter = std::env::var("RUST_LOG")
        .map_err(|_| ())
        .and_then(|rust_log| simplelog::LevelFilter::from_str(&rust_log).map_err(|_| ()))
        .unwrap_or(simplelog::LevelFilter::Info);

    let config = simplelog::ConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Right)
        .set_target_level(simplelog::LevelFilter::Off)
        .set_thread_level(simplelog::LevelFilter::Info)
        .set_thread_mode(simplelog::ThreadLogMode::Names)
        .set_time_format_rfc2822()
        .build();

    simplelog::TermLogger::init(
        level_filter,
        config,
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .expect("failed to initialize termlogger");
}
