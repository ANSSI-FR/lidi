// measure encoding & send performance in parallel with multiple threads

use diode::protocol::Header;
use human_bytes::human_bytes;
use std::net::{Ipv4Addr, UdpSocket};
use std::time::Instant;

use diode::{
    protocol::object_transmission_information, send::encoding::Encoding, test::build_random_message,
};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// size of udp write buffer
    #[arg(short, long, default_value_t = 60_000)]
    data_block_size: usize,

    #[arg(short, long, default_value_t = 6000)]
    repair_block_size: usize,

    #[arg(short, long, default_value_t = 1500)]
    mtu: usize,

    #[arg(short, long, default_value_t = 1)]
    threads: usize,

    #[arg(short, long, default_value_t = 100_000)] // around 10s of test
    block_count: usize,
}

fn process(encoding: Encoding, tx_socket: UdpSocket, max_count: u64, payload: Vec<u8>) {
    let mut counter = 0;
    while counter < max_count {
        let block_id = 0;
        let packets = encoding.encode(payload.clone(), block_id);
        packets.iter().for_each(|packet| {
            tx_socket.send(packet.data()).unwrap();
        });

        counter += 1;
    }
}

pub fn main() {
    let args = Args::parse();
    // init

    // transmission propreties, set by user
    let mtu = args.mtu as _;
    let block_size = args.data_block_size as _;
    let repair_block_size = args.repair_block_size as _;
    let max_count = args.block_count as u64;
    let thread_count = args.threads as _;

    // create configuration based on user configuration
    let object_transmission_info = object_transmission_information(mtu, block_size);

    // create our sockets
    let _rx_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 8888)).unwrap();

    // create our encoding module
    let encoding = Encoding::new(object_transmission_info, repair_block_size);

    let real_data_size =
        object_transmission_info.transfer_length() as usize - Header::serialize_overhead();
    let (_header, payload) = build_random_message(real_data_size);

    // now bench encoding performance
    let now = Instant::now();

    // start our injection thread
    let mut threads = vec![];
    (0..thread_count).for_each(|_| {
        let message = payload.clone();
        let encoding = encoding.clone();
        let tx_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        tx_socket.connect((Ipv4Addr::LOCALHOST, 8888)).unwrap();

        let thread = std::thread::spawn(move || process(encoding, tx_socket, max_count, message));
        threads.push(thread);
    });

    threads
        .into_iter()
        .for_each(|thread| thread.join().unwrap());

    let elapsed = now.elapsed().as_secs_f64();

    let max_count = max_count * thread_count;

    let transfer_length = object_transmission_info.transfer_length();
    let data_encoded = max_count * transfer_length;
    let data_rate = data_encoded as f64 / elapsed;

    let human_data_encoded = human_bytes(data_encoded as f64);
    let human_data_rate = human_bytes(data_rate as f64);

    println!(
        "{max_count} encode/send of {transfer_length} bytes, {human_data_encoded} encoded and sent in {elapsed:.2}s : {human_data_rate}/s",
    );
}
