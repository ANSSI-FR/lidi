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

pub fn init_logger(level_filter: log::LevelFilter) {
    let config = simplelog::ConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Right)
        .set_target_level(simplelog::LevelFilter::Off)
        .set_thread_level(level_filter)
        .set_thread_mode(simplelog::ThreadLogMode::Names)
        .set_time_format_rfc2822()
        .set_time_offset_to_local()
        .unwrap_or_else(|e| e)
        .build();

    if let Err(e) = simplelog::TermLogger::init(
        level_filter,
        config,
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    ) {
        eprintln!("failed to initialize logger: {e}");
    }
}
