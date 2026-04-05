use clap::Parser;
#[cfg(feature = "tls")]
use lidi_clients::tls;
#[cfg(feature = "unix")]
use std::os::unix;
use std::{
    io::{self, Read},
    net, path, thread,
};

#[allow(clippy::struct_field_names)]
#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct Clients {
    #[clap(
        value_name = "ip:port",
        long,
        help = "TCP address and port accepting connections from lidi-receive"
    )]
    from_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "ip:port",
        long,
        help = "TLS address and port accepting connections from lidi-receive"
    )]
    from_tls: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path to Unix socket accepting connections from lidi-receive"
    )]
    from_unix: Option<path::PathBuf>,
    #[clap(long, help = "Stdin")]
    from_stdin: bool,
}

#[derive(Parser)]
#[clap(about = "Receive and discard data from lidi-receive or lidi-oneshot-receive.")]
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
    from: Clients,
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

    if let Some(from_tcp) = args.from.from_tcp {
        #[cfg(not(feature = "tcp"))]
        {
            let _ = from_tcp;
            log::error!("TCP was not enabled at compilation");
        }
        #[cfg(feature = "tcp")]
        {
            log::debug!("TCP accepting on {from_tcp}");
            let server = net::TcpListener::bind(from_tcp).expect("TCP bind");
            loop {
                let (client, client_addr) = server.accept().expect("TCP accept");
                let id = client_addr.to_string();
                let buffer_size = args.buffer_size;
                thread::spawn(move || start(client, &id, buffer_size));
            }
        }
    } else if let Some(from_tls) = args.from.from_tls {
        #[cfg(not(feature = "tls"))]
        {
            let _ = from_tls;
            log::error!("TLS was not enabled at compilation");
        }
        #[cfg(feature = "tls")]
        {
            log::debug!("TLS accepting on {from_tls}");
            let server = tls::TcpListener::bind(&args.tls, &from_tls).expect("TLS/TCP bind");
            loop {
                let (client, client_addr) =
                    server.accept().expect("TCP accept").expect("TLS accept");
                let id = client_addr.to_string();
                let buffer_size = args.buffer_size;
                thread::spawn(move || start(client, &id, buffer_size));
            }
        }
    } else if let Some(from_unix) = args.from.from_unix {
        #[cfg(not(feature = "unix"))]
        {
            let _ = from_unix;
            log::error!("Unix was not enabled at compilation");
        }
        #[cfg(feature = "unix")]
        {
            log::debug!("Unix accepting on {}", from_unix.display());
            let server = unix::net::UnixListener::bind(&from_unix).expect("Unix bind");
            loop {
                let (client, client_addr) = server.accept().expect("Unix accept");
                let id = client_addr.as_pathname().map_or_else(
                    || from_unix.display().to_string(),
                    |path| path.display().to_string(),
                );
                let buffer_size = args.buffer_size;
                thread::spawn(move || start(client, &id, buffer_size));
            }
        }
    } else if args.from.from_stdin {
        let client = io::stdin();
        start(client, "stdin", args.buffer_size);
    } else {
        unreachable!();
    }
}

fn start<C>(mut client: C, id: &str, buffer_size: usize)
where
    C: Read,
{
    let mut buffer = vec![0u8; buffer_size];

    log::info!("client {id}: accepted connection");
    loop {
        let read = client
            .read(&mut buffer)
            .unwrap_or_else(|e| panic!("client {id} read: {e}"));
        log::debug!("client {id}: received {read} bytes");
        if read == 0 {
            log::info!("client {id}: end of connection");
            return;
        }
    }
}
