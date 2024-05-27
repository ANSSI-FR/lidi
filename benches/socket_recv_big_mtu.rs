// measure encoder performance
mod profiler;

use human_bytes::human_bytes;
use std::{net::Ipv4Addr, time::Instant};

use criterion::{criterion_group, criterion_main, Criterion};

use std::net::UdpSocket;

use std::sync::mpsc::{self, TryRecvError};

pub fn criterion_benchmark(c: &mut Criterion) {
    // transmission propreties, set by user
    const BLOCK_SIZE: usize = 8900;
    // create a thread to send datagram
    // channel to stop the thread
    let (tx, rx) = mpsc::channel();

    let rx_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 8888)).unwrap();

    // start it
    let thread = std::thread::spawn(move || {
        let tx_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        tx_socket.connect((Ipv4Addr::LOCALHOST, 8888)).unwrap();

        let mut counter = 0;
        loop {
            let buf = [0u8; BLOCK_SIZE];
            tx_socket.send(&buf).unwrap();

            counter += 1;
            if counter % 100000 == 0 {
                match rx.try_recv() {
                    Ok(_) | Err(TryRecvError::Disconnected) => {
                        println!("Terminating.");
                        break;
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }
        }
    });

    let mut counter = 0;

    let mut buf = [0u8; BLOCK_SIZE];

    let now = Instant::now();

    c.bench_function("socket_recv_big_mtu", |b| {
        b.iter(|| {
            let _len = rx_socket.recv(&mut buf).unwrap();
            counter += 1;
        });
    });

    let elapsed = now.elapsed().as_secs_f64();

    let data_sent = counter * BLOCK_SIZE;
    let data_rate = data_sent as f64 / elapsed;

    let human_data_encoded = human_bytes(data_sent as f64);
    let human_data_rate = human_bytes(data_rate);

    println!(
        "{counter} datagram of {BLOCK_SIZE} bytes, {human_data_encoded} received in {elapsed:.2}s : {human_data_rate}/s",
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
