use clap::Parser;
#[cfg(feature = "tls")]
use lidi_clients::tls;
use rand::TryRng;
#[cfg(feature = "unix")]
use std::os::unix;
use std::{
    io::{self, Write},
    net, path,
};

#[allow(clippy::struct_field_names)]
#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct Clients {
    #[clap(
        value_name = "ip:port",
        long,
        help = "TCP address and port to connect to lidi-send"
    )]
    to_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "ip:port",
        long,
        help = "TLS address and port to connect to lidi-send"
    )]
    to_tls: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path to Unix socket to connect to lidi-send"
    )]
    to_unix: Option<path::PathBuf>,
    #[clap(long, help = "Stdout")]
    to_stdout: bool,
}

#[derive(Parser)]
#[clap(about = "Send random data to lidi-send or lidi-oneshot-send.")]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Off|Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(long, help = "Path to log4rs configuration file")]
    log_config: Option<path::PathBuf>,
    #[clap(flatten)]
    to: Clients,
    #[clap(
        default_value = "4194304",
        value_name = "bytes",
        long,
        help = "Size of client internal read/write buffer"
    )]
    buffer_size: usize,
    #[clap(flatten)]
    tls: lidi_clients::Tls,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = lidi_clients::init_logger(args.log_level, args.log_config.as_ref()) {
        eprintln!("failed to initialize logger: {e}");
        return;
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    if let Some(to_tcp) = args.to.to_tcp {
        #[cfg(not(feature = "tcp"))]
        {
            let _ = to_tcp;
            log::error!("TCP was not enabled at compilation");
        }
        #[cfg(feature = "tcp")]
        {
            log::debug!("TCP connect to {to_tcp}");
            let diode = net::TcpStream::connect(to_tcp).expect("TCP connect");
            start(diode, args.buffer_size);
        }
    } else if let Some(to_tls) = args.to.to_tls {
        #[cfg(not(feature = "tls"))]
        {
            let _ = to_tls;
            log::error!("TLS was not enabled at compilation");
        }
        #[cfg(feature = "tls")]
        {
            log::debug!("TLS connect to {to_tls}");
            let context = tls::ClientContext::try_from(&args.tls).expect("TLS config");
            let diode = tls::TcpStream::connect(&context, &to_tls).expect("TLS connect");
            start(diode, args.buffer_size);
        }
    } else if let Some(to_unix) = args.to.to_unix {
        #[cfg(not(feature = "unix"))]
        {
            let _ = to_unix;
            log::error!("Unix was not enabled at compilation");
        }
        #[cfg(feature = "unix")]
        {
            log::debug!("Unix connect to {}", to_unix.display());
            let diode = unix::net::UnixStream::connect(to_unix).expect("Unix connect");
            start(diode, args.buffer_size);
        }
    } else if args.to.to_stdout {
        let diode = io::stdout();
        start(diode, args.buffer_size);
    } else {
        unreachable!();
    }
}

fn start<D>(mut diode: D, buffer_size: usize)
where
    D: Write,
{
    let mut rng = rand::rngs::SysRng;
    let mut buffer = vec![0u8; buffer_size];
    rng.try_fill_bytes(&mut buffer).unwrap();

    let mut rnd = (rng.try_next_u32().unwrap() & 0xff) as u8;
    let step = (rng.try_next_u32().unwrap() & 0xff) as u8;

    loop {
        for b in &mut buffer {
            *b ^= rnd;
        }
        log::debug!("sending buffer of {buffer_size} bytes");
        diode.write_all(&buffer).expect("write");
        rnd = rnd.wrapping_add(step);
    }
}
