use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(feature = "decoders")]
fn benchmark_file_source_conversion(c: &mut Criterion) {
    use std::borrow::Cow;

    use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Channels, SampleBuffer, Signal};
    use symphonia::core::conv::IntoSample;

    let frames = 1024;
    let mut buf = AudioBuffer::<f32>::new(
        frames as u64,
        symphonia::core::audio::SignalSpec::new(
            44100,
            Channels::FRONT_LEFT | Channels::FRONT_RIGHT,
        ),
    );
    buf.render_reserved(Some(frames));
    for c in 0..2 {
        let chan = buf.chan_mut(c);
        #[allow(clippy::needless_range_loop)]
        for f in 0..frames {
            chan[f] = ((f + c) % 32000) as f32 / 32000.0;
        }
    }

    let audio_ref = AudioBufferRef::F32(Cow::Owned(buf));

    // Existing approach
    c.bench_function("audio_buffer_to_vec_f32_old", |b| {
        b.iter(|| {
            let mut out: Vec<i16> = Vec::new();
            if let AudioBufferRef::F32(ref buf) = audio_ref {
                for frame in 0..buf.frames() {
                    for channel in 0..buf.spec().channels.count() {
                        let sample = buf.chan(channel)[frame].into_sample();
                        out.push(sample);
                    }
                }
            }
            out
        })
    });

    // New optimized approach using symphonia's SampleBuffer
    c.bench_function("audio_buffer_to_vec_f32_optimized", |b| {
        b.iter(|| {
            let mut out: Vec<i16> = Vec::new();
            if let AudioBufferRef::F32(ref buf) = audio_ref {
                let channels = buf.spec().channels.count();
                let frames = buf.frames();
                out.reserve(frames * channels);

                let mut sample_buf = SampleBuffer::<i16>::new(buf.capacity() as u64, *buf.spec());
                sample_buf.copy_interleaved_ref(AudioBufferRef::F32(buf.clone()));
                out.extend_from_slice(sample_buf.samples());
            }
            out
        })
    });

    // New optimized approach without SampleBuffer (just pre-reserve + fast iteration)
    c.bench_function("audio_buffer_to_vec_f32_optimized_loop", |b| {
        b.iter(|| {
            let mut out: Vec<i16> = Vec::new();
            if let AudioBufferRef::F32(ref buf) = audio_ref {
                let channels = buf.spec().channels.count();
                let frames = buf.frames();
                out.reserve(frames * channels);
                for frame in 0..frames {
                    for channel in 0..channels {
                        out.push(buf.chan(channel)[frame].into_sample());
                    }
                }
            }
            out
        })
    });
}

#[cfg(not(feature = "decoders"))]
fn benchmark_file_source_conversion(_c: &mut Criterion) {}

criterion_group!(benches, benchmark_file_source_conversion);
criterion_main!(benches);
