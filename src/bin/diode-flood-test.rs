use clap::{Arg, Command};
use rand::RngCore;
use std::io::Write;
use std::net::TcpStream;
use std::{env, net::SocketAddr, str::FromStr};

fn main() {
    let args = Command::new(env!("CARGO_BIN_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("to_tcp")
                .long("to_tcp")
                .value_name("ip:port")
                .default_value("127.0.0.1:5000")
                .help("Address and port to connect to diode-send"),
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

    let to_tcp = SocketAddr::from_str(args.get_one::<String>("to_tcp").expect("default"))
        .expect("invalid to_tcp parameter");
    let buffer_size = *args.get_one::<usize>("buffer_size").expect("default");

    init_logger();

    log::debug!("connecting to {}", to_tcp);
    let mut diode = TcpStream::connect(to_tcp).expect("tcp connect");
    diode
        .shutdown(std::net::Shutdown::Read)
        .expect("tcp read shutdown");

    let mut thread_rng = rand::thread_rng();
    let mut buffer = vec![0u8; buffer_size];
    thread_rng.fill_bytes(&mut buffer);

    loop {
        let rnd = (thread_rng.next_u32() & 0xff) as u8;
        for n in buffer.iter_mut() {
            *n ^= rnd;
        }
        log::debug!("sending buffer of {buffer_size} bytes");
        diode.write_all(&buffer).expect("tcp write");
    }
}

fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env()
    } else {
        simple_logger::init_with_level(log::Level::Info)
    }
    .expect("logger initialization")
}
