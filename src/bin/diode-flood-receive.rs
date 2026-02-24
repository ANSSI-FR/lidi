use clap::Parser;
use std::{
    io::{self, Read},
    net,
    os::unix,
    path, thread,
};

#[allow(clippy::struct_field_names)]
#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct Clients {
    #[clap(
        value_name = "ip:port",
        long,
        help = "TCP address and port accepting connections from diode-receive"
    )]
    from_tcp: Option<net::SocketAddr>,
    #[clap(
        value_name = "path",
        long,
        help = "Path to Unix socket accepting connections from diode-receive"
    )]
    from_unix: Option<path::PathBuf>,
    #[clap(long, help = "Stdin")]
    from_stdin: bool,
}

#[derive(Parser)]
#[clap(about = "Receive and discard data from diode-receive or diode-oneshot-receive.")]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Off|Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(flatten)]
    from: Clients,
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

    if let Err(e) = diode::init_logger(args.log_level, None, false) {
        eprintln!("failed to initialize logger: {e}");
        return;
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    if let Some(from_tcp) = args.from.from_tcp {
        log::debug!("TCP accepting on {from_tcp}");
        let server = net::TcpListener::bind(from_tcp).expect("TCP bind");
        loop {
            let (client, addr) = server.accept().expect("TCP accept");
            let id = addr.to_string();
            let buffer_size = args.buffer_size;
            thread::spawn(move || start(client, &id, buffer_size));
        }
    } else if let Some(from_unix) = args.from.from_unix {
        log::debug!("Unix accepting on {}", from_unix.display());
        let server = unix::net::UnixListener::bind(&from_unix).expect("Unix bind");
        loop {
            let (client, addr) = server.accept().expect("Unix accept");
            let id = addr.as_pathname().map_or_else(
                || from_unix.display().to_string(),
                |path| path.display().to_string(),
            );
            let buffer_size = args.buffer_size;
            thread::spawn(move || start(client, &id, buffer_size));
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

    log::info!("{id} client: accepted connection");
    loop {
        let read = client.read(&mut buffer).expect(id);
        log::debug!("{id} client: received {read} bytes");
        if read == 0 {
            log::info!("{id} client: end of connection");
            return;
        }
    }
}
