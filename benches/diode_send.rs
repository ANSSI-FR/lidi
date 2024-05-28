// measure complete send performance in one thread
mod profiler;

use diode::send::tcp;
use human_bytes::human_bytes;
use std::io::Write;
use std::net::{Ipv4Addr, UdpSocket};
use std::str::FromStr;
use std::sync::mpsc::{self, TryRecvError};
use std::{net, time::Instant};

use criterion::{criterion_group, criterion_main, Criterion};

use diode::{
    protocol::object_transmission_information, send::encoding::Encoding, test::build_random_data,
};

pub fn criterion_benchmark(c: &mut Criterion) {
    // init

    // transmission propreties, set by user
    let mtu = 1500;
    let block_size = 60000;
    let repair_block_size = 6000;

    // transmission propreties, set by user
    let from_tcp = net::SocketAddr::from_str("0.0.0.0:5000").unwrap();

    // create configuration based on user configuration
    let object_transmission_info = object_transmission_information(mtu, block_size);

    let real_data_size = object_transmission_info.transfer_length() as usize;
    let buffer = build_random_data(real_data_size);

    // prepare tcp socket
    log::info!("accepting TCP clients at {}", from_tcp);

    let tcp_listener = match net::TcpListener::bind(from_tcp) {
        Err(e) => {
            log::error!("failed to bind TCP {}: {}", from_tcp, e);
            return;
        }
        Ok(listener) => listener,
    };

    // create a thread to send datagram
    // channel to stop the thread
    let (tx, rx) = mpsc::channel();

    // start our injection thread
    let thread = std::thread::spawn(move || {
        let mut tx_socket = net::TcpStream::connect((Ipv4Addr::LOCALHOST, 5000)).unwrap();

        let timeout = Some(std::time::Duration::from_secs(1));
        if let Err(e) = tx_socket.set_write_timeout(timeout) {
            log::error!("failed to set client read timeout: {e}");
        }

        loop {
            // send data without header
            if let Err(e) = tx_socket.write_all(&buffer) {
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    panic!("Error tcp write: {e}");
                }
            }

            // check if we must stop
            match rx.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    println!("Terminating.");
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    });

    // accept our new client
    let (client, _sockaddr) = tcp_listener.accept().unwrap();
    let mut tcp = tcp::Tcp::new(client, real_data_size as _, 0, None);
    if let Err(e) = tcp.configure() {
        log::warn!("client: error: {e}");
    }

    // prepare udp socket to send
    let tx_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    tx_socket.connect((Ipv4Addr::LOCALHOST, 8888)).unwrap();

    let _rx_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 8888)).unwrap();

    // create our encoding module
    let encoding = Encoding::new(object_transmission_info, repair_block_size);

    // now bench encoding performance
    let now = Instant::now();
    let mut counter = 0;

    c.bench_function("decoding", |b| {
        b.iter(|| {
            // loop

            match tcp.read() {
                Ok(ret) => {
                    if let Some((_header, payload)) = ret {
                        // encode one block
                        let block_id = 0;
                        let packets = encoding.encode(payload, block_id);

                        packets.iter().for_each(|packet| {
                            tx_socket.send(packet.data()).unwrap();
                        });
                    }
                }

                Err(e) => log::error!("client: error: {e}"),
            }

            counter += 1;
        });
    });

    tcp.shutdown().unwrap();

    let elapsed = now.elapsed().as_secs_f64();

    let transfer_length = object_transmission_info.transfer_length();
    let data_encoded = counter * transfer_length;
    let data_rate = data_encoded as f64 / elapsed;

    let human_data_encoded = human_bytes(data_encoded as f64);
    let human_data_rate = human_bytes(data_rate as f64);

    println!(
        "{counter} encoding of {transfer_length} bytes, {human_data_encoded} encoded in {elapsed:.2}s : {human_data_rate}/s",
    );

    tx.send(()).unwrap();
    thread.join().unwrap();
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(profiler::FlamegraphProfiler::new(100));
    targets = criterion_benchmark
}
criterion_main!(benches);
