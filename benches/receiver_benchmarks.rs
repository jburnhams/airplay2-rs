//! Performance benchmarks for receiver components

use criterion::{criterion_group, criterion_main, Criterion, black_box};
use airplay2::audio::jitter::{JitterBuffer, JitterBufferConfig};
use airplay2::receiver::rtp_receiver::AudioPacket;
use std::time::Instant;

fn jitter_buffer_insert(c: &mut Criterion) {
    c.bench_function("jitter_insert", |b| {
        let config = JitterBufferConfig::default();
        let mut buffer = JitterBuffer::new(config);

        let mut seq = 0u16;

        b.iter(|| {
            let packet = AudioPacket {
                sequence: seq,
                timestamp: seq as u32 * 352,
                ssrc: 0x12345678,
                audio_data: vec![0u8; 1408],
                received_at: Instant::now(),
            };

            buffer.insert(black_box(packet));
            seq = seq.wrapping_add(1);
        });
    });
}

fn jitter_buffer_pop(c: &mut Criterion) {
    c.bench_function("jitter_pop", |b| {
        let config = JitterBufferConfig {
            min_depth: 10,
            target_depth: 50,
            max_depth: 200,
            ..Default::default()
        };
        let mut buffer = JitterBuffer::new(config);

        // Fill buffer
        for seq in 0..100u16 {
            buffer.insert(AudioPacket {
                sequence: seq,
                timestamp: seq as u32 * 352,
                ssrc: 0x12345678,
                audio_data: vec![0u8; 1408],
                received_at: Instant::now(),
            });
        }

        b.iter(|| {
            let _ = black_box(buffer.pop());
        });
    });
}

fn rtp_header_parse(c: &mut Criterion) {
    c.bench_function("rtp_parse", |b| {
        let packet = vec![
            0x80, 0x60,
            0x00, 0x01,
            0x00, 0x00, 0x01, 0x60,
            0x12, 0x34, 0x56, 0x78,
        ];

        b.iter(|| {
            use airplay2::protocol::rtp::RtpHeader;
            let _ = black_box(RtpHeader::decode(&packet));
        });
    });
}

criterion_group!(
    benches,
    jitter_buffer_insert,
    jitter_buffer_pop,
    rtp_header_parse,
);

criterion_main!(benches);
