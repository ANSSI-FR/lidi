// measure encoder performance
mod profiler;

use human_bytes::human_bytes;
use std::time::Instant;

use criterion::{criterion_group, criterion_main, Criterion};

use diode::{
    protocol::object_transmission_information, receive::decoding::Decoding,
    send::encoding::Encoding, test::build_random_message,
};

pub fn criterion_benchmark(c: &mut Criterion) {
    // transmission propreties, set by user
    let mtu = 1500;
    let block_size = 60000;
    let repair_block_size = 6000;

    // create configuration based on user configuration
    let object_transmission_info = object_transmission_information(mtu, block_size);

    let message = build_random_message(object_transmission_info);

    // create our encoding module
    let encoding = Encoding::new(object_transmission_info, repair_block_size);

    // encode one block
    let block_id = 0;
    let packets = encoding.encode(message, block_id);

    // prepare decoding
    let decoder = Decoding::new(object_transmission_info);

    // now bench encoding performance
    let now = Instant::now();
    let mut counter = 0;

    c.bench_function("decoding", |b| {
        b.iter(|| {
            decoder.decode(packets.clone(), block_id);
            counter += 1;
        });
    });

    let elapsed = now.elapsed().as_secs_f64();

    let transfer_length = object_transmission_info.transfer_length();
    let data_encoded = counter * transfer_length;
    let data_rate = data_encoded as f64 / elapsed;

    let human_data_encoded = human_bytes(data_encoded as f64);
    let human_data_rate = human_bytes(data_rate as f64);

    println!(
        "{counter} decoding of {transfer_length} bytes, {human_data_encoded} decoded in {elapsed:.2}s : {human_data_rate}/s",
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(profiler::FlamegraphProfiler::new(100));
    targets = criterion_benchmark
}
criterion_main!(benches);
