//! Adaptive Jitter Buffer for `AirPlay` 2
//!
//! Buffers audio frames to handle network jitter and enable
//! synchronized multi-room playback.

use super::rtp_receiver::AudioFrame;
use std::collections::BTreeMap;

/// Jitter buffer configuration
#[derive(Debug, Clone)]
pub struct JitterBufferConfig {
    /// Minimum buffer depth (ms)
    pub min_depth_ms: u32,
    /// Maximum buffer depth (ms)
    pub max_depth_ms: u32,
    /// Target buffer depth (ms)
    pub target_depth_ms: u32,
    /// Sample rate
    pub sample_rate: u32,
    /// Channels
    pub channels: u8,
}

impl Default for JitterBufferConfig {
    fn default() -> Self {
        Self {
            min_depth_ms: 50,
            max_depth_ms: 2000,
            target_depth_ms: 200,
            sample_rate: 44100,
            channels: 2,
        }
    }
}

/// Adaptive jitter buffer
pub struct JitterBuffer {
    config: JitterBufferConfig,
    /// Frames indexed by RTP timestamp
    frames: BTreeMap<u32, AudioFrame>,
    /// Current playback position (RTP timestamp)
    playback_position: u32,
    /// Buffer state
    state: BufferState,
    /// Statistics
    stats: BufferStats,
    /// Last sequence seen
    last_sequence: Option<u16>,
    /// Playback started flag
    started: bool,
    /// Currently playing frame
    current_frame: Option<AudioFrame>,
    /// Offset within the current frame (in samples, i.e., vector index)
    current_frame_offset: usize,
}

/// State of the jitter buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferState {
    /// Initial buffering
    Buffering,
    /// Normal playback
    Playing,
    /// Underrun - need more data
    Underrun,
    /// Overflow - discarding old data
    Overflow,
}

/// Statistics collected by the jitter buffer
#[derive(Debug, Default)]
pub struct BufferStats {
    /// Total frames received
    pub frames_received: u64,
    /// Total frames successfully played
    pub frames_played: u64,
    /// Frames dropped due to overflow
    pub frames_dropped: u64,
    /// Frames lost (sequence gaps or late arrival)
    pub frames_lost: u64,
    /// Number of underrun events
    pub underruns: u64,
    /// Number of overflow events
    pub overflows: u64,
    /// Current buffer depth in milliseconds
    pub current_depth_ms: u32,
    /// Estimated network jitter in milliseconds
    pub jitter_estimate_ms: f32,
}

impl JitterBuffer {
    /// Create a new jitter buffer with the given configuration
    #[must_use]
    pub fn new(config: JitterBufferConfig) -> Self {
        Self {
            config,
            frames: BTreeMap::new(),
            playback_position: 0,
            state: BufferState::Buffering,
            stats: BufferStats::default(),
            last_sequence: None,
            started: false,
            current_frame: None,
            current_frame_offset: 0,
        }
    }

    /// Add a frame to the buffer
    pub fn push(&mut self, frame: AudioFrame) {
        self.stats.frames_received += 1;

        // Check for sequence gaps
        if let Some(last_seq) = self.last_sequence {
            let expected = last_seq.wrapping_add(1);
            if frame.sequence != expected {
                let gap = frame.sequence.wrapping_sub(expected);
                if gap < 100 {
                    // Reasonable gap
                    self.stats.frames_lost += u64::from(gap);
                }
            }
        }
        self.last_sequence = Some(frame.sequence);

        // Insert frame
        self.frames.insert(frame.timestamp, frame);

        // Update buffer depth
        self.update_depth();

        // Check for overflow
        if self.stats.current_depth_ms > self.config.max_depth_ms {
            self.handle_overflow();
        }

        // Transition from buffering to playing
        if self.state == BufferState::Buffering
            && self.stats.current_depth_ms >= self.config.target_depth_ms
        {
            self.state = BufferState::Playing;
            self.started = true;
            tracing::info!(
                "Jitter buffer: starting playback at {}ms depth",
                self.stats.current_depth_ms
            );
        }
    }

    /// Get samples for playback
    ///
    /// Returns `sample_count` *frames* (e.g. 352), resulting in `sample_count * channels` samples.
    pub fn pull(&mut self, sample_count: usize) -> Vec<i16> {
        let channels = self.config.channels as usize;
        let total_samples_needed = sample_count * channels;

        if self.state == BufferState::Buffering {
            // Return silence while buffering
            return vec![0i16; total_samples_needed];
        }

        let mut output = Vec::with_capacity(total_samples_needed);

        while output.len() < total_samples_needed {
            let remaining_output_capacity = total_samples_needed - output.len();

            // Check if we have a current frame with remaining samples
            if let Some(ref frame) = self.current_frame {
                let available_in_frame = frame.samples.len() - self.current_frame_offset;
                let take = available_in_frame.min(remaining_output_capacity);

                output.extend_from_slice(
                    &frame.samples[self.current_frame_offset..self.current_frame_offset + take],
                );
                self.current_frame_offset += take;

                // If frame exhausted
                if self.current_frame_offset >= frame.samples.len() {
                    // Advance playback position by frame duration (samples / channels)
                    // Assuming frame.samples.len() is multiple of channels
                    #[allow(clippy::cast_possible_truncation)]
                    let frame_duration = (frame.samples.len() / channels) as u32;
                    self.playback_position = self.playback_position.wrapping_add(frame_duration);

                    self.current_frame = None;
                    self.current_frame_offset = 0;
                    self.stats.frames_played += 1;
                }
            } else {
                // Try to get next frame at playback_position
                if let Some(frame) = self.frames.remove(&self.playback_position) {
                    self.current_frame = Some(frame);
                    self.current_frame_offset = 0;
                    // Loop continues and will consume from current_frame
                } else {
                    // Missing frame - concealment
                    // We don't have a frame, so we don't know its size.
                    // Assume standard size (352) for concealment advancement, or just fill needed?
                    // If we fill needed, we might desync if the hole was exactly 352 but we needed 100.
                    // But `playback_position` must match frame timestamps.
                    // If we are missing frame at T, we assume it existed and had typical duration.
                    // Standard AirPlay 2 ALAC frame is 352 samples.

                    self.stats.frames_lost += 1;

                    // Generate silence for concealment.
                    // How much? We need `remaining_output_capacity` samples?
                    // But we also need to advance `playback_position` correctly.
                    // Let's assume a "virtual" missing frame of 352 samples duration.
                    let concealment_frames = 352;
                    let concealment_samples = concealment_frames * channels;

                    // We can return as much silence as needed from this "virtual frame",
                    // or just fill the output request and assume we are "playing" silence until the next real frame?
                    // But we need to align with next real frame timestamp.

                    // Simplest approach: Assume the missing frame was 352 samples long.
                    // Generate silence for what we need from it (up to 352 samples),
                    // and advance playback_position by 352.
                    // If we need more than 352, we'll hit this again for the next frame.

                    // Wait, if we only need 100 samples, and we decide the missing frame was 352 samples long.
                    // We return 100 samples of silence.
                    // We should technically "keep" the remaining 252 samples of silence for the next pull?
                    // This implies we should create a "Silence Frame".

                    let silence_vec = vec![0i16; concealment_samples];
                    let silence_frame = AudioFrame {
                        sequence: 0, // Dummy
                        timestamp: self.playback_position,
                        samples: silence_vec,
                        receive_time: std::time::Instant::now(),
                    };

                    self.current_frame = Some(silence_frame);
                    self.current_frame_offset = 0;
                    // Loop continues and consumes silence
                }
            }
        }

        // Check for underrun
        if self.frames.is_empty() && self.started {
            self.state = BufferState::Underrun;
            self.stats.underruns += 1;
            tracing::warn!("Jitter buffer underrun");
        }

        self.update_depth();
        output
    }

    /// Flush buffer and reset to initial state
    pub fn flush(&mut self) {
        self.frames.clear();
        self.current_frame = None;
        self.current_frame_offset = 0;
        self.state = BufferState::Buffering;
        self.started = false;
        tracing::debug!("Jitter buffer flushed");
    }

    /// Flush to specific RTP timestamp
    pub fn flush_to(&mut self, timestamp: u32) {
        self.frames.retain(|&ts, _| ts >= timestamp);
        if let Some(ref frame) = self.current_frame {
            if frame.timestamp < timestamp {
                self.current_frame = None;
                self.current_frame_offset = 0;
            }
        }
        self.playback_position = timestamp;
        tracing::debug!("Jitter buffer flushed to timestamp {}", timestamp);
    }

    /// Set playback position for synchronized start
    pub fn set_playback_position(&mut self, timestamp: u32) {
        self.playback_position = timestamp;
        // Also clear current frame if it doesn't match?
        // Usually called before start.
        self.current_frame = None;
        self.current_frame_offset = 0;
    }

    fn update_depth(&mut self) {
        let channels = self.config.channels as usize;

        // Determine the "end" timestamp (timestamp of the last sample + 1)
        let end_ts = if let Some((_, last_frame)) = self.frames.iter().next_back() {
            #[allow(clippy::cast_possible_truncation)]
            let duration = (last_frame.samples.len() / channels) as u32;
            last_frame.timestamp.wrapping_add(duration)
        } else if let Some(ref frame) = self.current_frame {
            #[allow(clippy::cast_possible_truncation)]
            let duration = (frame.samples.len() / channels) as u32;
            frame.timestamp.wrapping_add(duration)
        } else {
            // Buffer empty
            self.stats.current_depth_ms = 0;
            return;
        };

        // Determine the "current" timestamp (playhead)
        let current_ts = if self.state == BufferState::Buffering {
            // In buffering, depth is relative to the first frame
            if let Some((first_ts, _)) = self.frames.iter().next() {
                *first_ts
            } else {
                // Should be covered by empty check above, but safe fallback
                self.stats.current_depth_ms = 0;
                return;
            }
        } else {
            // In playing/underrun, depth is relative to playback position
            // If we have a current frame, we are at playback_position + offset
            #[allow(clippy::cast_possible_truncation)]
            let offset_duration = (self.current_frame_offset / channels) as u32;
            self.playback_position.wrapping_add(offset_duration)
        };

        let depth_samples = end_ts.wrapping_sub(current_ts);

        #[allow(clippy::cast_possible_truncation, reason = "Depth fits in u32")]
        {
            self.stats.current_depth_ms =
                (u64::from(depth_samples) * 1000 / u64::from(self.config.sample_rate)) as u32;
        }
    }

    fn handle_overflow(&mut self) {
        self.stats.overflows += 1;
        tracing::warn!(
            "Jitter buffer overflow at {}ms",
            self.stats.current_depth_ms
        );

        // Remove oldest frames until at target
        while self.stats.current_depth_ms > self.config.target_depth_ms {
            if let Some((&ts, _)) = self.frames.iter().next() {
                self.frames.remove(&ts);
                self.stats.frames_dropped += 1;
            } else {
                break;
            }
            self.update_depth();
        }

        self.state = BufferState::Playing;
    }

    /// Get current buffer state
    #[must_use]
    pub fn state(&self) -> BufferState {
        self.state
    }

    /// Get buffer statistics
    #[must_use]
    pub fn stats(&self) -> &BufferStats {
        &self.stats
    }

    /// Get current buffer depth in milliseconds
    #[must_use]
    pub fn depth_ms(&self) -> u32 {
        self.stats.current_depth_ms
    }
}
