use diode::{
    file::{self, send::send_file},
    init_logger,
};
use inotify::{Inotify, WatchMask};
use std::{net, path::PathBuf, str::FromStr};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct SendFileConfig {
    /// IP address and port to connect in TCP to diode-send (ex "127.0.0.1:5001")
    #[arg(long, default_value_t = String::from("127.0.0.1:5001"))]
    to_tcp: String,
    /// Size of file buffer
    #[arg(long, default_value_t = 8196)]
    buffer_size: usize,
    /// Compute a hash of file content (default is false)
    #[arg(long, default_value_t = false)]
    hash: bool,
    /// Directory containing files to send
    #[arg()]
    dir: String,
    /// maximum number of files to send per session
    #[arg(long)]
    maximum_files: Option<usize>,
    /// Path to log configuration file
    #[arg(short, long)]
    log_config: Option<String>,
    /// Verbosity level. Using it multiple times adds more logs.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,
}

fn watch_files(
    config: &file::Config,
    inotify: &mut Inotify,
    dir: &str,
    maximum_files: Option<usize>,
) {
    let mut count = 0;

    log::info!("connecting to {}", config.diode);
    let mut diode = net::TcpStream::connect(config.diode).expect("Cannot connect to diode");

    // Read events that were added with `Watches::add` above.
    let mut buffer = [0; 1024];

    let mut last_file = false;

    loop {
        // ça marche pas ça
        let events = inotify
            .read_events_blocking(&mut buffer)
            .expect("Error while reading events");

        for event in events {
            count += 1;
            if let Some(maximum_files) = maximum_files {
                if count >= maximum_files {
                    // quit this loop to force a reconnect
                    last_file = true;
                }
            }
            log::debug!("new event {event:?}");
            // Handle event
            if let Some(osstr) = event.name {
                let filename = osstr.to_string_lossy().to_string();
                let mut path = PathBuf::from(dir);
                path.push(&filename);
                match send_file(config, &mut diode, path.to_str().unwrap(), last_file) {
                    Ok(total) => {
                        log::info!("{filename} sent, {total} bytes");
                    }
                    Err(e) => {
                        log::warn!("Unable to send {filename}: {e}");
                    }
                }

                if let Err(e) = std::fs::remove_file(path) {
                    log::warn!("Unable to delete {filename}: {e}");
                }
            }

            if last_file {
                return;
            }
        }
    }
}

fn main() {
    let args = SendFileConfig::parse();

    init_logger(args.log_config.as_ref(), args.debug);

    let to_tcp =
        net::SocketAddr::from_str(&args.to_tcp).expect("to-tcp must be of the form ip:port");
    let buffer_size = args.buffer_size;
    let hash = args.hash;

    let config = file::Config {
        diode: to_tcp,
        buffer_size,
        hash,
    };

    let mut inotify = Inotify::init().expect("Error while initializing inotify instance");

    // Watch for modify and close events.
    inotify
        .watches()
        .add(
            args.dir.as_str(),
            WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO,
        )
        .expect("Failed to add file watch");

    loop {
        watch_files(&config, &mut inotify, args.dir.as_str(), args.maximum_files);
    }
}
