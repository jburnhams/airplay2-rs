use airplay2::protocol::crypto::Aes128Ctr;
use airplay2::protocol::plist::{PlistValue, decode, encode};
use airplay2::protocol::rtp::RtpCodec;
use airplay2::protocol::rtp::packet_buffer::{BufferedPacket, PacketBuffer, PacketLossDetector};
use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::collections::HashMap;

fn packet_buffer_benchmark(c: &mut Criterion) {
    let mut buffer = PacketBuffer::new(PacketBuffer::DEFAULT_SIZE);

    // Fill buffer
    for i in 0..PacketBuffer::DEFAULT_SIZE {
        buffer.push(BufferedPacket {
            sequence: i as u16,
            timestamp: i as u32 * 352,
            data: vec![0u8; 10], // Small payload
        });
    }

    let mut group = c.benchmark_group("packet_buffer");

    for count in [10, 50, 100].iter() {
        group.bench_with_input(
            criterion::BenchmarkId::new("get_range", count),
            count,
            |b, &count| {
                b.iter(|| {
                    buffer
                        .get_range(black_box(0), black_box(count as u16))
                        .count()
                })
            },
        );
    }
    group.finish();
}

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

fn rtp_encoding_benchmark(c: &mut Criterion) {
    let mut codec = RtpCodec::new(1234);
    let key = [0u8; 32];
    codec.set_chacha_encryption(key);
    let payload = vec![0u8; 352 * 4];
    let mut output = Vec::with_capacity(2048);

    c.bench_function("rtp_encode_chacha", |b| {
        b.iter(|| {
            output.clear();
            let _ = codec.encode_arbitrary_payload(black_box(&payload), &mut output);
        })
    });
}

fn packet_loss_detector_benchmark(c: &mut Criterion) {
    c.bench_function("packet_loss_detector_gaps", |b| {
        let mut detector = PacketLossDetector::new();
        // Initialize
        detector.process(0);
        let mut seq: u16 = 0;

        b.iter(|| {
            // Advance by 10 to create a gap of 9 packets
            seq = seq.wrapping_add(10);
            detector.process(black_box(seq))
        })
    });
}

criterion_group!(
    benches,
    plist_benchmark,
    crypto_benchmark,
    rtsp_encoding_benchmark,
    rtp_encoding_benchmark,
    packet_buffer_benchmark,
    packet_loss_detector_benchmark
);
criterion_main!(benches);
