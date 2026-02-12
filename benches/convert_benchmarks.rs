use airplay2::audio::ChannelConfig;
use airplay2::audio::convert::{convert_channels, convert_channels_into};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn benchmark_convert_channels(c: &mut Criterion) {
    let frames = 1024;

    // Stereo to Mono
    let input_stereo: Vec<f32> = vec![0.5; frames * 2];
    c.bench_function("convert_channels_stereo_to_mono", |b| {
        b.iter(|| {
            convert_channels(
                black_box(&input_stereo),
                ChannelConfig::Stereo,
                ChannelConfig::Mono,
            )
        })
    });

    // Mono to Stereo
    let input_mono: Vec<f32> = vec![0.5; frames];
    c.bench_function("convert_channels_mono_to_stereo", |b| {
        b.iter(|| {
            convert_channels(
                black_box(&input_mono),
                ChannelConfig::Mono,
                ChannelConfig::Stereo,
            )
        })
    });

    // Stereo to Mono (into)
    let mut output_mono: Vec<f32> = Vec::with_capacity(frames);
    c.bench_function("convert_channels_into_stereo_to_mono", |b| {
        b.iter(|| {
            convert_channels_into(
                black_box(&input_stereo),
                ChannelConfig::Stereo,
                ChannelConfig::Mono,
                black_box(&mut output_mono),
            )
        })
    });

    // Mono to Stereo (into)
    let mut output_stereo: Vec<f32> = Vec::with_capacity(frames * 2);
    c.bench_function("convert_channels_into_mono_to_stereo", |b| {
        b.iter(|| {
            convert_channels_into(
                black_box(&input_mono),
                ChannelConfig::Mono,
                ChannelConfig::Stereo,
                black_box(&mut output_stereo),
            )
        })
    });
}

criterion_group!(benches, benchmark_convert_channels);
criterion_main!(benches);
