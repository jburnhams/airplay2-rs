use airplay2::protocol::raop::RaopSessionKeys;
use airplay2::streaming::{RaopStreamConfig, RaopStreamer};
use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};

fn raop_encoding_benchmark(c: &mut Criterion) {
    // 1. Setup
    let keys = RaopSessionKeys::generate().expect("Failed to generate session keys");
    let config = RaopStreamConfig::default();
    let mut streamer = RaopStreamer::new(&keys, config);

    // Typical ALAC frame payload (352 samples stereo 16-bit is 1408 bytes)
    let payload_size = 1408;
    let audio_data = vec![0xAB; payload_size];

    let mut group = c.benchmark_group("raop_encoding");
    group.throughput(Throughput::Bytes(payload_size as u64));

    group.bench_function("encode_frame", |b| {
        b.iter(|| {
            // Encode the frame (includes packetization and encryption)
            let _ = streamer.encode_frame(black_box(&audio_data));
        })
    });

    group.finish();
}

criterion_group!(benches, raop_encoding_benchmark);
criterion_main!(benches);
