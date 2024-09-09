// measure encoder performance
mod profiler;

use human_bytes::human_bytes;
use std::time::Instant;

use criterion::{criterion_group, criterion_main, Criterion};

use diode::{
    protocol::object_transmission_information, send::encoding::Encoding, test::build_random_message,
};

pub fn criterion_benchmark(c: &mut Criterion) {
    // transmission propreties, set by user
    let mtu = 1500;
    let block_size = 60000;
    let repair_block_size = 6000;

    // create configuration based on user configuration
    let object_transmission_info = object_transmission_information(mtu, block_size);

    // create our encoding module
    let encoding = Encoding::new(object_transmission_info, repair_block_size);

    let real_data_size = object_transmission_info.transfer_length() as usize;
    let (_header, payload) = build_random_message(real_data_size);

    // now bench encoding performance
    let now = Instant::now();
    let mut counter = 0;

    c.bench_function("encoding", |b| {
        b.iter(|| {
            // TODO : find what is this block id
            let block_id = 0;
            encoding.encode(payload.clone(), block_id);

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
        "{counter} encoding of {transfer_length} bytes, {human_data_encoded} encoded in {elapsed:.2}s : {human_data_rate}/s",
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(profiler::FlamegraphProfiler::new(100));
    targets = criterion_benchmark
}
criterion_main!(benches);
