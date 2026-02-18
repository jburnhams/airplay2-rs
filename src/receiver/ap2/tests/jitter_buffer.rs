use crate::receiver::ap2::jitter_buffer::{BufferState, JitterBuffer, JitterBufferConfig};
use crate::receiver::ap2::rtp_receiver::AudioFrame;
use std::time::Instant;

fn make_frame(seq: u16, ts: u32) -> AudioFrame {
    AudioFrame {
        sequence: seq,
        timestamp: ts,
        samples: vec![0i16; 704], // 352 stereo samples
        receive_time: Instant::now(),
    }
}

#[test]
fn test_buffering_to_playing() {
    let config = JitterBufferConfig {
        target_depth_ms: 100,
        sample_rate: 44100,
        ..Default::default()
    };
    let mut buffer = JitterBuffer::new(config);

    // Add frames until target depth reached
    for i in 0..20 {
        buffer.push(make_frame(i, u32::from(i) * 352));
    }

    assert_eq!(buffer.state(), BufferState::Playing);
}

#[test]
fn test_sequence_gap_detection() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig::default());

    buffer.push(make_frame(1, 352));
    buffer.push(make_frame(5, 352 * 5)); // Gap of 3 frames (2, 3, 4)

    assert_eq!(buffer.stats().frames_lost, 3);
}

#[test]
fn test_underrun_detection() {
    let config = JitterBufferConfig {
        target_depth_ms: 20, // Low target for quick start
        sample_rate: 44100,
        ..Default::default()
    };
    let mut buffer = JitterBuffer::new(config);

    // Push enough frames to start playing
    for i in 0..5 {
        buffer.push(make_frame(i, u32::from(i) * 352));
    }
    assert_eq!(buffer.state(), BufferState::Playing);

    // Pull all frames
    for _ in 0..5 {
        let samples = buffer.pull(352);
        assert_eq!(samples.len(), 352 * 2);
    }

    // Next pull should cause underrun
    let _ = buffer.pull(352);
    assert_eq!(buffer.state(), BufferState::Underrun);
    assert!(buffer.stats().underruns >= 1);
}

#[test]
fn test_overflow_handling() {
    let config = JitterBufferConfig {
        max_depth_ms: 100, // Small max depth
        target_depth_ms: 50,
        sample_rate: 44100,
        ..Default::default()
    };
    let mut buffer = JitterBuffer::new(config);

    // Push many frames to exceed max_depth_ms (100ms) at some point.
    // Each frame is ~8ms. 20 frames = ~160ms total pushed.
    // Overflow triggers when depth > 100ms.
    // When triggered, it drops frames until depth <= 50ms.
    // Subsequent pushes will increase depth again.
    for i in 0..20 {
        buffer.push(make_frame(i, u32::from(i) * 352));
    }

    // Should have overflowed at least once
    assert!(buffer.stats().overflows > 0);

    // Depth should be within max limits (<= 100)
    // It might be higher than target (50) because we kept pushing after overflow.
    assert!(buffer.depth_ms() <= 100, "Depth is {}", buffer.depth_ms());

    // Ensure frames were dropped to handle the overflow
    assert!(buffer.stats().frames_dropped > 0);
}

#[test]
fn test_flush() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig::default());

    buffer.push(make_frame(1, 352));
    buffer.flush();

    assert_eq!(buffer.state(), BufferState::Buffering);
    // Depth calc depends on first frame or playback position.
    // Flush clears everything, so frames empty, current_frame None.
    // update_depth sets to 0.
    assert_eq!(buffer.depth_ms(), 0);
}

#[test]
fn test_pull_silence_when_buffering() {
    let mut buffer = JitterBuffer::new(JitterBufferConfig::default());

    // State is Buffering initially
    assert_eq!(buffer.state(), BufferState::Buffering);

    let samples = buffer.pull(352);
    // Should be silence (zeros)
    assert_eq!(samples.len(), 352 * 2);
    assert!(samples.iter().all(|&s| s == 0));
}

#[test]
fn test_pull_concealment() {
    let config = JitterBufferConfig {
        target_depth_ms: 0, // Instant play for testing
        sample_rate: 44100,
        ..Default::default()
    };
    let mut buffer = JitterBuffer::new(config);

    buffer.set_playback_position(352);

    let mut frame1 = make_frame(1, 352);
    frame1.samples.fill(1); // Fill with 1s
    buffer.push(frame1);

    let mut frame3 = make_frame(3, 352 * 3);
    frame3.samples.fill(3); // Fill with 3s
    buffer.push(frame3);

    // Pull frame 1
    let samples1 = buffer.pull(352);
    assert!(samples1.iter().all(|&s| s == 1));

    // Pull frame 2 (missing)
    let samples2 = buffer.pull(352);
    assert_eq!(samples2.len(), 352 * 2);
    // Should be silence (concealment)
    assert!(samples2.iter().all(|&s| s == 0));
    assert!(buffer.stats().frames_lost > 0);

    // Pull frame 3
    let samples3 = buffer.pull(352);
    assert!(samples3.iter().all(|&s| s == 3));
}

#[test]
fn test_partial_reads() {
    let config = JitterBufferConfig {
        target_depth_ms: 0,
        sample_rate: 44100,
        ..Default::default()
    };
    let mut buffer = JitterBuffer::new(config);

    buffer.set_playback_position(352);

    // Push one frame (352 frames = 704 samples)
    let mut frame = make_frame(1, 352);
    frame.samples.fill(1);
    buffer.push(frame);

    // Pull 100 frames (200 samples)
    let samples1 = buffer.pull(100);
    assert_eq!(samples1.len(), 200);
    assert!(samples1.iter().all(|&s| s == 1));

    // Pull remaining 252 frames (504 samples)
    let samples2 = buffer.pull(252);
    assert_eq!(samples2.len(), 504);
    assert!(samples2.iter().all(|&s| s == 1));

    // Frame should be consumed now. Next pull should be silence (concealment).
    let samples3 = buffer.pull(100);
    assert_eq!(samples3.len(), 200);
    assert!(samples3.iter().all(|&s| s == 0));
    assert!(buffer.stats().frames_lost > 0);
}

#[test]
fn test_depth_accuracy_with_partial_read() {
    let config = JitterBufferConfig {
        target_depth_ms: 0,
        sample_rate: 44100,
        ..Default::default()
    };
    let mut buffer = JitterBuffer::new(config);

    buffer.set_playback_position(352);

    // Push two frames: 352 (length 352*2 samples) and 352*2 (length 352*2 samples)
    // Frame duration is 352 samples.
    buffer.push(make_frame(1, 352));
    buffer.push(make_frame(2, 352 * 2));

    // Depth should be 2 frames (704 samples) ~15.9ms
    let expected_depth = (2 * 352 * 1000 / 44100) as u32;
    assert_eq!(buffer.depth_ms(), expected_depth, "Initial depth mismatch");

    // Pull half of the first frame (176 frames = 352 samples)
    let _ = buffer.pull(176);

    // Remaining depth should be 1.5 frames (528 samples) ~11.9ms
    let expected_depth_partial = (528 * 1000 / 44100) as u32;
    // Allow slight rounding diff
    assert!(
        (buffer.depth_ms() as i32 - expected_depth_partial as i32).abs() <= 1,
        "Depth after partial read mismatch: got {}, expected {}",
        buffer.depth_ms(),
        expected_depth_partial
    );

    // Pull remaining half of first frame
    let _ = buffer.pull(176);

    // Remaining depth should be 1 frame (352 samples) ~7.9ms
    let expected_depth_one = (352 * 1000 / 44100) as u32;
    assert_eq!(buffer.depth_ms(), expected_depth_one, "Depth after full frame 1 read mismatch");
}
