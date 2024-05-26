// measure encoder performance
mod profiler;

use human_bytes::human_bytes;
use std::time::Instant;

use criterion::{criterion_group, criterion_main, Criterion};

use diode::protocol::{object_transmission_information, Message, MessageType};
use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

pub fn criterion_benchmark(c: &mut Criterion) {
    // set a seed for random algorithm generation
    let mut rng = XorShiftRng::from_seed([
        3, 42, 93, 129, 1, 85, 72, 42, 84, 23, 95, 212, 253, 10, 4, 2,
    ]);

    // transmission propreties, set by user
    let mtu = 1500;
    let block_size = 60000;
    let repair_block_size = 6000;

    // create configuration based on user configuration
    let object_transmission_info = object_transmission_information(mtu, block_size);

    // get real transfer size (algorithm constraint)
    let block_size = object_transmission_info.transfer_length() as usize;

    // create our encoding module
    let encoding =
        diode::send::encoding::Encoding::new(object_transmission_info, repair_block_size);

    // get real transfer data size ( remove message header overhead )
    let real_data_size = block_size - Message::serialize_overhead();

    // generate some random data
    let data = (0..real_data_size)
        .map(|_| rng.gen_range(0..=255) as u8)
        .collect::<Vec<_>>();

    // now bench encoding performance
    let now = Instant::now();
    let mut counter = 0;

    c.bench_function("encoding", |b| {
        b.iter(|| {
            // TODO : find what is this block id
            let block_id = 0;
            let message = Message::new(MessageType::Data, data.len() as _, 0, Some(&data));
            encoding.encode(message, block_id);

            counter += 1;
        });
    });

    let elapsed = now.elapsed().as_secs_f64();

    let data_encoded = counter * real_data_size;
    let data_rate = data_encoded as f64 / elapsed;

    let human_data_encoded = human_bytes(data_encoded as f64);
    let human_data_rate = human_bytes(data_rate as f64);

    println!(
        "{counter} encoding of {real_data_size} bytes, {human_data_encoded} encoded in {elapsed:.2}s : {human_data_rate}/s",
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(profiler::FlamegraphProfiler::new(100));
    targets = criterion_benchmark
}
criterion_main!(benches);
