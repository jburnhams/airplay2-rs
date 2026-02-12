use airplay2::audio::{AudioFormat, ChannelConfig, SampleFormat};
use airplay2::streaming::{ResamplingSource, SilenceSource};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::io::Read;

fn resampler_benchmark(c: &mut Criterion) {
    let input_format = AudioFormat {
        sample_rate: airplay2::audio::SampleRate::Hz44100,
        sample_format: SampleFormat::I16,
        channels: ChannelConfig::Stereo,
    };
    let output_format = AudioFormat {
        sample_rate: airplay2::audio::SampleRate::Hz48000,
        sample_format: SampleFormat::I16,
        channels: ChannelConfig::Stereo,
    };

    c.bench_function("resample_44100_to_48000", |b| {
        b.iter_with_setup(
            || {
                let source = SilenceSource::new(input_format);
                ResamplingSource::new(source, output_format).unwrap()
            },
            |mut resampler: ResamplingSource<SilenceSource>| {
                let mut buffer = vec![0u8; 4096];
                let _ = black_box(resampler.read(&mut buffer).unwrap());
            },
        );
    });
}

criterion_group!(benches, resampler_benchmark);
criterion_main!(benches);
