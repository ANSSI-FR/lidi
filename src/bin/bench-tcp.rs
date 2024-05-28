use clap::Parser;
use std::{
    io::{Read, Write},
    net,
    str::FromStr,
};

use rand::RngCore;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Sender mode
    #[arg(short, long)]
    to_tcp: Option<String>,

    /// Receiver mode
    #[arg(short, long)]
    bind_tcp: Option<String>,
}

fn main() {
    let args = Args::parse();

    if let Some(to_tcp) = args.to_tcp {
        let to_tcp =
            net::SocketAddr::from_str(&to_tcp).expect("to_tcp must be of the form ip:port");

        let mut tx_socket = net::TcpStream::connect(to_tcp).expect("can't connect to tcp socket");

        let mut data = [0u8; 256000];
        rand::thread_rng().fill_bytes(&mut data);

        loop {
            tx_socket.write_all(&data).expect("can't send data");
        }
    }

    if let Some(bind_tcp) = args.bind_tcp {
        let server = net::TcpListener::bind(bind_tcp).expect("can't bind socket");

        let mut data = vec![0; 256000];

        loop {
            let (mut client, _client_addr) = server.accept().expect("can't accept client");
            while let Ok(len) = client.read(&mut data[..]) {
                if len == 0 {
                    break;
                }
            }
        }
    }
}
