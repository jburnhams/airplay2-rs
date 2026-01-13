use criterion::{Criterion, criterion_group, criterion_main};

fn rtsp_encoding_benchmark(c: &mut Criterion) {
    // TODO: Add benchmarks when RTSP codec is implemented
    c.bench_function("rtsp_encode_request", |b| {
        b.iter(|| {
            // Benchmark code here
        })
    });
}

criterion_group!(benches, rtsp_encoding_benchmark);
criterion_main!(benches);
