use clap::{Arg, ArgGroup, Command};
use rand::RngCore;
use std::{env, io::Write, net, os::unix, path, str::FromStr};

fn main() {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("to_tcp")
                .long("to_tcp")
                .value_name("ip:port")
                .help("TCP address and port to connect to diode-send"),
        )
        .arg(
            Arg::new("to_unix")
                .long("to_unix")
                .value_name("path")
                .help("Path to Unix socket to connect to diode-send"),
        )
        .group(
            ArgGroup::new("to")
                .required(true)
                .args(["to_tcp", "to_unix"]),
        )
        .arg(
            Arg::new("buffer_size")
                .long("buffer_size")
                .value_name("nb_bytes")
                .default_value("4194304") // 4096 * 1024
                .value_parser(clap::value_parser!(usize))
                .help("Size of file read/TCP write buffer"),
        )
        .get_matches();

    let to_tcp = args
        .get_one::<String>("to_tcp")
        .map(|s| net::SocketAddr::from_str(s).expect("to_tcp must be of the form ip:port"));
    let to_unix = args
        .get_one::<String>("to_unix")
        .map(|s| path::PathBuf::from_str(s).expect("to_unix must point to a valid path"));
    let buffer_size = *args.get_one::<usize>("buffer_size").expect("default");

    diode::init_logger();

    if let Some(to_tcp) = to_tcp {
        log::debug!("TCP connect to {}", to_tcp);
        let diode = net::TcpStream::connect(to_tcp).expect("TCP connect");
        start(diode, buffer_size)
    } else {
        let to_unix = to_unix.expect("to_tcp and to_unix are mutually exclusive");
        log::debug!("Unix connect to {}", to_unix.display());
        let diode = unix::net::UnixStream::connect(to_unix).expect("Unix connect");
        start(diode, buffer_size)
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
        for n in buffer.iter_mut() {
            *n ^= rnd;
        }
        log::debug!("sending buffer of {buffer_size} bytes");
        diode.write_all(&buffer).expect("write");
    }
}
