use airplay2::protocol::crypto::Aes128Ctr;
use airplay2::protocol::plist::{PlistValue, decode, encode};
use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::collections::HashMap;

fn plist_benchmark(c: &mut Criterion) {
    // 1. Prepare data
    // Create a reasonably complex plist
    let mut dict = HashMap::new();
    dict.insert(
        "StringKey".to_string(),
        PlistValue::String("Some string value".to_string()),
    );
    dict.insert("IntKey".to_string(), PlistValue::Integer(12345));
    dict.insert("BoolKey".to_string(), PlistValue::Boolean(true));
    dict.insert(
        "ArrayKey".to_string(),
        PlistValue::Array(vec![
            PlistValue::Integer(1),
            PlistValue::Integer(2),
            PlistValue::Integer(3),
        ]),
    );
    // Nested dict
    let mut inner = HashMap::new();
    inner.insert(
        "InnerKey".to_string(),
        PlistValue::String("InnerValue".to_string()),
    );
    dict.insert("DictKey".to_string(), PlistValue::Dictionary(inner));

    let value = PlistValue::Dictionary(dict);
    let encoded = encode(&value).unwrap();

    // 2. Benchmarks
    c.bench_function("plist_decode_complex", |b| {
        b.iter(|| decode(black_box(&encoded)).unwrap())
    });

    c.bench_function("plist_encode_complex", |b| {
        b.iter(|| encode(black_box(&value)).unwrap())
    });
}

fn crypto_benchmark(c: &mut Criterion) {
    let key = [0u8; 16];
    let iv = [0u8; 16];
    let mut cipher = Aes128Ctr::new(&key, &iv).unwrap();

    let size = 1024 * 16; // 16KB buffer
    let mut data = vec![0u8; size];

    let mut group = c.benchmark_group("aes_ctr");
    group.throughput(Throughput::Bytes(size as u64));

    group.bench_function("encrypt_16k", |b| {
        b.iter(|| {
            // CTR is stateful, so typically we'd reset, but for pure throughput
            // continuously processing is fine or we can clone.
            // Resetting seek is cheaper.
            cipher.seek(0);
            cipher.apply_keystream(black_box(&mut data));
        })
    });
    group.finish();
}

fn rtsp_encoding_benchmark(c: &mut Criterion) {
    // Stub for now until we have RTSP types available here
    c.bench_function("rtsp_encode_request_stub", |b| {
        b.iter(|| {
            black_box(1 + 1);
        })
    });
}

criterion_group!(
    benches,
    plist_benchmark,
    crypto_benchmark,
    rtsp_encoding_benchmark
);
criterion_main!(benches);
