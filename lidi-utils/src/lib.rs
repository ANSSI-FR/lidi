pub mod config;
#[cfg(feature = "hash")]
pub mod hash;
pub mod socket;

/// # Errors
///
/// Will return `Err` if `file` cannot be opened
/// or logger cannot be set (Term or file mode).
pub fn init_logger(level_filter: log::LevelFilter, stderr_only: bool) -> Result<(), String> {
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

    simplelog::TermLogger::init(
        level_filter,
        config,
        terminal_mode,
        simplelog::ColorChoice::Auto,
    )
    .map_err(|e| e.to_string())
}
