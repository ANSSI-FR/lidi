use clap::Parser;
use rand::RngCore;
use std::{io::Write, net, os::unix, path};

#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct Clients {
    #[clap(
        value_name = "ip:port",
        long,
        help = "TCP address and port to connect to diode-send"
    )]
    to_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path to Unix socket to connect to diode-send"
    )]
    to_unix: Option<path::PathBuf>,
}

#[derive(Parser)]
#[clap(about = "Send random data to diode-send or diode-oneshot-send.")]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Off|Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(flatten)]
    to: Clients,
    #[clap(
        default_value = "4194304",
        value_name = "bytes",
        long,
        help = "Size of client internal read/write buffer"
    )]
    buffer_size: usize,
}

fn main() {
    let args = Args::parse();

    diode::init_logger(args.log_level, false);

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    if let Some(to_tcp) = args.to.to_tcp {
        log::debug!("TCP connect to {to_tcp}");
        let diode = net::TcpStream::connect(to_tcp).expect("TCP connect");
        start(diode, args.buffer_size);
    } else if let Some(to_unix) = args.to.to_unix {
        log::debug!("Unix connect to {}", to_unix.display());
        let diode = unix::net::UnixStream::connect(to_unix).expect("Unix connect");
        start(diode, args.buffer_size);
    } else {
        unreachable!();
    }
}

fn start<D>(mut diode: D, buffer_size: usize)
where
    D: Write,
{
    let mut thread_rng = rand::rng();
    let mut buffer = vec![0u8; buffer_size];
    thread_rng.fill_bytes(&mut buffer);

    loop {
        let rnd = (thread_rng.next_u32() & 0xff) as u8;
        for n in &mut buffer {
            *n ^= rnd;
        }
        log::debug!("sending buffer of {buffer_size} bytes");
        diode.write_all(&buffer).expect("write");
    }
}
