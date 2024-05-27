// measure encoder performance
mod profiler;

use human_bytes::human_bytes;
use std::{net::Ipv4Addr, time::Instant};

use criterion::{criterion_group, criterion_main, Criterion};

use std::net::UdpSocket;

pub fn criterion_benchmark(c: &mut Criterion) {
    let _rx_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 8888)).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));

    // transmission propreties, set by user
    const BLOCK_SIZE: usize = 1460;

    let mut counter = 0;

    let tx_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    tx_socket.connect((Ipv4Addr::LOCALHOST, 8888)).unwrap();

    let buf = [0u8; BLOCK_SIZE];

    let now = Instant::now();

    c.bench_function("socket_send", |b| {
        b.iter(|| {
            tx_socket.send(&buf).unwrap();
            counter += 1;
        });
    });

    let elapsed = now.elapsed().as_secs_f64();

    let data_sent = counter * BLOCK_SIZE;
    let data_rate = data_sent as f64 / elapsed;

    let human_data_encoded = human_bytes(data_sent as f64);
    let human_data_rate = human_bytes(data_rate);

    println!(
        "{counter} datagram of {BLOCK_SIZE} bytes, {human_data_encoded} sent in {elapsed:.2}s : {human_data_rate}/s",
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(profiler::FlamegraphProfiler::new(100));
    targets = criterion_benchmark
}
criterion_main!(benches);
