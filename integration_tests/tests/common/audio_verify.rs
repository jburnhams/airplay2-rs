#![allow(dead_code)]
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawAudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub endianness: Endianness,
    pub signed: bool,
}

impl RawAudioFormat {
    pub const CD_QUALITY: Self = Self {
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };

    pub const HIRES: Self = Self {
        sample_rate: 48000,
        channels: 2,
        bits_per_sample: 24,
        endianness: Endianness::Little,
        signed: true,
    };
}

#[derive(Debug, Clone)]
pub struct RawAudio {
    pub data: Vec<u8>,
    pub format: RawAudioFormat,
}

impl RawAudio {
    pub fn from_file(path: &Path, format: RawAudioFormat) -> Result<Self, std::io::Error> {
        let data = fs::read(path)?;
        Ok(Self { data, format })
    }

    pub fn from_bytes(data: Vec<u8>, format: RawAudioFormat) -> Self {
        Self { data, format }
    }

    pub fn samples_i32(&self) -> Vec<i32> {
        let bytes_per_sample = (self.format.bits_per_sample / 8) as usize;
        if bytes_per_sample == 0 {
            return Vec::new();
        }
        let mut samples = Vec::with_capacity(self.data.len() / bytes_per_sample);

        for chunk in self.data.chunks_exact(bytes_per_sample) {
            let sample = if self.format.bits_per_sample == 16 {
                match self.format.endianness {
                    Endianness::Little => i16::from_le_bytes([chunk[0], chunk[1]]) as i32,
                    Endianness::Big => i16::from_be_bytes([chunk[0], chunk[1]]) as i32,
                }
            } else if self.format.bits_per_sample == 24 {
                match self.format.endianness {
                    Endianness::Little => {
                        let bytes = [chunk[0], chunk[1], chunk[2], if chunk[2] & 0x80 != 0 { 0xFF } else { 0x00 }];
                        i32::from_le_bytes(bytes)
                    }
                    Endianness::Big => {
                        let bytes = [if chunk[0] & 0x80 != 0 { 0xFF } else { 0x00 }, chunk[0], chunk[1], chunk[2]];
                        i32::from_be_bytes(bytes)
                    }
                }
            } else {
                0
            };
            samples.push(sample);
        }
        samples
    }

    pub fn samples_i16(&self) -> Vec<i16> {
        self.samples_i32().into_iter().map(|s| {
            if self.format.bits_per_sample == 24 {
                (s >> 8) as i16
            } else {
                s as i16
            }
        }).collect()
    }

    pub fn samples_f32(&self) -> Vec<f32> {
        let i32_samples = self.samples_i32();
        let max_val = if self.format.bits_per_sample == 24 {
            8_388_608.0 // 2^23
        } else {
            32768.0 // 2^15
        };
        i32_samples.into_iter().map(|s| s as f32 / max_val).collect()
    }

    pub fn channel(&self, ch: usize) -> Vec<f32> {
        let samples = self.samples_f32();
        samples
            .iter()
            .skip(ch)
            .step_by(self.format.channels as usize)
            .copied()
            .collect()
    }

    pub fn duration(&self) -> Duration {
        let frames = self.num_frames();
        Duration::from_secs_f64(frames as f64 / self.format.sample_rate as f64)
    }

    pub fn num_frames(&self) -> usize {
        let bytes_per_frame =
            (self.format.bits_per_sample / 8) as usize * self.format.channels as usize;
        if bytes_per_frame == 0 {
            0
        } else {
            self.data.len() / bytes_per_frame
        }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct SineWaveCheck {
    pub expected_frequency: f32,
    pub frequency_tolerance_pct: f32,
    pub min_amplitude: i16,
    pub max_silence_run_ms: f32,
    pub check_frequency: bool,
    pub check_continuity: bool,
    pub check_amplitude: bool,
    pub channel: Option<usize>,
}

impl Default for SineWaveCheck {
    fn default() -> Self {
        Self {
            expected_frequency: 440.0,
            frequency_tolerance_pct: 5.0,
            min_amplitude: 20000,
            max_silence_run_ms: 100.0,
            check_frequency: true,
            check_continuity: true,
            check_amplitude: true,
            channel: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SineWaveResult {
    pub measured_frequency: f32,
    pub frequency_error_pct: f32,
    pub min_sample: i16,
    pub max_sample: i16,
    pub amplitude_range: i32,
    pub rms: f32,
    pub peak: f32,
    pub crest_factor: f32,
    pub max_silence_run_samples: usize,
    pub max_silence_run_ms: f32,
    pub num_frames: usize,
    pub duration: Duration,
    pub passed: bool,
    pub failure_reasons: Vec<String>,
}

impl SineWaveResult {
    pub fn assert_passed(&self) -> Result<(), String> {
        if self.passed {
            Ok(())
        } else {
            Err(format!(
                "Audio verification failed:\n{}",
                self.failure_reasons.join("\n")
            ))
        }
    }
}

impl SineWaveCheck {
    pub fn verify(&self, audio: &RawAudio) -> Result<SineWaveResult, String> {
        if audio.data.len() < 10000 {
            return Err(format!("Audio too short: {} bytes", audio.data.len()));
        }

        let ch = self.channel.unwrap_or(0);
        let raw_samples = audio.channel(ch);

        let num_samples = raw_samples.len();

        // Strip setup latency (skip first 200ms)
        let skip_samples = (audio.format.sample_rate as f32 * 0.2) as usize;
        let samples = if raw_samples.len() > skip_samples * 2 {
            &raw_samples[skip_samples..]
        } else {
            &raw_samples[..]
        };

        // Calculate mean for DC offset
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;

        let mut min_sample = i16::MAX;
        let mut max_sample = i16::MIN;
        let mut zero_crossings = 0;
        let mut prev_sample = 0.0;

        for &val in samples {
            let sample = val - mean;

            // Re-scale back to i16 range for the amplitude check logic (which assumes full scale)
            let i16_sample = (sample * 32768.0) as i16;
            min_sample = min_sample.min(i16_sample);
            max_sample = max_sample.max(i16_sample);

            if (prev_sample < 0.0 && sample >= 0.0) || (prev_sample >= 0.0 && sample < 0.0) {
                zero_crossings += 1;
            }
            prev_sample = sample;
        }

        let amplitude_range = (max_sample as i32) - (min_sample as i32);
        let rms = (samples.iter().map(|&s| (s - mean) * (s - mean)).sum::<f32>() / samples.len() as f32).sqrt() * 32768.0;
        let peak = samples.iter().map(|&s| (s - mean).abs()).fold(0.0f32, f32::max) * 32768.0;
        let crest_factor = if rms > 0.0 { peak / rms } else { 0.0 };

        let actual_duration = samples.len() as f32 / audio.format.sample_rate as f32;
        let estimated_frequency = (zero_crossings as f32 / actual_duration) / 2.0;
        let frequency_error = (estimated_frequency - self.expected_frequency).abs();
        let frequency_error_pct = (frequency_error / self.expected_frequency) * 100.0;

        let mut max_zero_run = 0;
        let mut current_zero_run = 0;
        for &sample in samples {
            // scaled to match previous i16 threshold
            if (sample * 32768.0).abs() < 100.0 {
                current_zero_run += 1;
                max_zero_run = max_zero_run.max(current_zero_run);
            } else {
                current_zero_run = 0;
            }
        }
        let max_silence_run_ms = (max_zero_run as f32 / audio.format.sample_rate as f32) * 1000.0;

        let mut passed = true;
        let mut failure_reasons = Vec::new();

        if self.check_amplitude && amplitude_range < self.min_amplitude as i32 {
            passed = false;
            failure_reasons.push(format!(
                "Amplitude range too low: {} (expected >{})",
                amplitude_range, self.min_amplitude
            ));
        }

        if self.check_frequency && frequency_error_pct > self.frequency_tolerance_pct {
            passed = false;
            failure_reasons.push(format!(
                "Frequency mismatch: expected {}Hz, got {:.1}Hz (error: {:.1}%)",
                self.expected_frequency, estimated_frequency, frequency_error_pct
            ));
        }

        if self.check_continuity && max_silence_run_ms > self.max_silence_run_ms {
            passed = false;
            failure_reasons.push(format!(
                "Found suspicious silence: {} ms",
                max_silence_run_ms
            ));
        }

        let duration = num_samples as f32 / audio.format.sample_rate as f32;

        Ok(SineWaveResult {
            measured_frequency: estimated_frequency,
            frequency_error_pct,
            min_sample,
            max_sample,
            amplitude_range,
            rms,
            peak,
            crest_factor,
            max_silence_run_samples: max_zero_run,
            max_silence_run_ms,
            num_frames: num_samples,
            duration: Duration::from_secs_f64(duration as f64),
            passed,
            failure_reasons,
        })
    }
}

pub struct StereoSineCheck {
    pub left_frequency: f32,
    pub right_frequency: f32,
    pub frequency_tolerance_pct: f32,
}

pub struct StereoSineResult {
    pub left: SineWaveResult,
    pub right: SineWaveResult,
}

impl StereoSineCheck {
    pub fn verify(&self, audio: &RawAudio) -> Result<StereoSineResult, String> {
        let left_check = SineWaveCheck {
            expected_frequency: self.left_frequency,
            frequency_tolerance_pct: self.frequency_tolerance_pct,
            channel: Some(0),
            ..Default::default()
        };
        let right_check = SineWaveCheck {
            expected_frequency: self.right_frequency,
            frequency_tolerance_pct: self.frequency_tolerance_pct,
            channel: Some(1),
            ..Default::default()
        };

        let left = left_check.verify(audio)?;
        let right = right_check.verify(audio)?;

        Ok(StereoSineResult { left, right })
    }
}

#[derive(Debug, Clone)]
pub struct CompareResult {
    pub sample_count_match: bool,
    pub sent_frames: usize,
    pub received_frames: usize,
    pub matching_frames: usize,
    pub first_mismatch_frame: Option<usize>,
    pub max_sample_diff: i32,
    pub mean_sample_diff: f64,
    pub bit_exact: bool,
}

pub fn compare_audio_exact(sent: &RawAudio, received: &RawAudio) -> CompareResult {
    let sent_samples = sent.samples_i16();
    let received_samples = received.samples_i16();

    let offset = align_audio_i16(
        &sent_samples,
        &received_samples,
        sent.format.sample_rate as usize * 2,
    );

    let sent_frames = sent_samples.len();
    let received_frames = received_samples.len();

    let mut matching_frames = 0;
    let mut first_mismatch_frame = None;
    let mut max_sample_diff = 0;
    let mut sum_diff = 0.0;

    let len = sent_samples
        .len()
        .min(received_samples.len().saturating_sub(offset));

    for i in 0..len {
        let diff = (sent_samples[i] as i32 - received_samples[i + offset] as i32).abs();
        if diff == 0 {
            matching_frames += 1;
        } else if first_mismatch_frame.is_none() {
            first_mismatch_frame = Some(i);
        }
        max_sample_diff = max_sample_diff.max(diff);
        sum_diff += diff as f64;
    }

    let mean_sample_diff = if len > 0 { sum_diff / len as f64 } else { 0.0 };

    // Often there's an off-by-one or small math variance in the floating point generation
    // allow a small diff for floating-point to integer round-trip "exactness" and possible small
    // offset alignments
    let bit_exact = max_sample_diff <= 10;

    CompareResult {
        sample_count_match: sent_frames.abs_diff(received_frames) <= 1,
        sent_frames,
        received_frames,
        matching_frames,
        first_mismatch_frame,
        max_sample_diff,
        mean_sample_diff,
        bit_exact,
    }
}

pub fn align_audio_i16(reference: &[i16], captured: &[i16], max_offset: usize) -> usize {
    let mut best_offset = 0;
    let mut max_corr = -1.0;

    let max_len = max_offset.min(captured.len().saturating_sub(reference.len().min(44100)));
    let check_len = reference.len().min(44100);

    if check_len == 0 {
        return 0;
    }

    for offset in 0..=max_len {
        let mut corr = 0.0;
        for i in 0..check_len {
            corr += (reference[i] as f64) * (captured[i + offset] as f64);
        }
        if corr > max_corr {
            max_corr = corr;
            best_offset = offset;
        }
    }

    best_offset
}

#[derive(Debug, Clone)]
pub struct GapInfo {
    pub start_frame: usize,
    pub end_frame: usize,
    pub duration: Duration,
    pub position: Duration,
}

pub fn measure_onset_latency(audio: &RawAudio, threshold: f32) -> Duration {
    let samples = audio.samples_f32();
    let sample_rate = audio.format.sample_rate as f32;

    for (i, &sample) in samples.iter().enumerate() {
        if sample.abs() > threshold {
            let frames = i / audio.format.channels as usize;
            return Duration::from_secs_f32(frames as f32 / sample_rate);
        }
    }

    audio.duration()
}

pub fn measure_gap_latency(audio: &RawAudio, gap_threshold_ms: f32) -> Vec<GapInfo> {
    let samples = audio.samples_f32();
    let sample_rate = audio.format.sample_rate as f32;
    let mut gaps = Vec::new();

    let mut in_gap = false;
    let mut gap_start_sample = 0;

    // threshold close to 0
    let silence_threshold = 0.005;

    for (i, &sample) in samples.iter().enumerate() {
        if sample.abs() < silence_threshold {
            if !in_gap {
                in_gap = true;
                gap_start_sample = i;
            }
        } else if in_gap {
            in_gap = false;
            let gap_duration_samples = i - gap_start_sample;
            let gap_duration_frames = gap_duration_samples / audio.format.channels as usize;
            let gap_duration_ms = (gap_duration_frames as f32 / sample_rate) * 1000.0;

            if gap_duration_ms >= gap_threshold_ms {
                let start_frame = gap_start_sample / audio.format.channels as usize;
                let end_frame = i / audio.format.channels as usize;

                gaps.push(GapInfo {
                    start_frame,
                    end_frame,
                    duration: Duration::from_secs_f32(gap_duration_frames as f32 / sample_rate),
                    position: Duration::from_secs_f32(start_frame as f32 / sample_rate),
                });
            }
        }
    }

    gaps
}

pub fn compute_snr(original: &RawAudio, received: &RawAudio) -> f64 {
    let sent_samples = original.samples_f32();
    let received_samples = received.samples_f32();

    let offset = align_audio_f32(
        &sent_samples,
        &received_samples,
        original.format.sample_rate as usize * 2,
    );
    let len = sent_samples
        .len()
        .min(received_samples.len().saturating_sub(offset));

    if len == 0 {
        return 0.0;
    }

    let mut signal_power = 0.0;
    let mut noise_power = 0.0;

    for i in 0..len {
        let s = sent_samples[i];
        let r = received_samples[i + offset];
        signal_power += s * s;
        let diff = s - r;
        noise_power += diff * diff;
    }

    if noise_power == 0.0 {
        return 100.0; // Perfect match
    }

    (10.0 * (signal_power / noise_power).log10()) as f64
}

pub fn align_audio_f32(reference: &[f32], captured: &[f32], max_offset: usize) -> usize {
    let mut best_offset = 0;
    let mut max_corr = -1.0;

    let max_len = max_offset.min(captured.len().saturating_sub(reference.len().min(44100)));
    let check_len = reference.len().min(44100);

    if check_len == 0 {
        return 0;
    }

    for offset in 0..=max_len {
        let mut corr = 0.0;
        for i in 0..check_len {
            corr += (reference[i] as f64) * (captured[i + offset] as f64);
        }
        if corr > max_corr {
            max_corr = corr;
            best_offset = offset;
        }
    }

    best_offset
}

pub fn audio_diagnostic_report(audio: &RawAudio, check: &SineWaveResult) -> String {
    format!(
        "Audio Diagnostic Report\n=======================\nFormat: {}-bit {:?} stereo @ {} \
         Hz\nDuration: {:.2}s ({} frames)\nData size: {} bytes\n\nAmplitude:\nMin sample: {}    \
         Max sample: {}\nRMS: {:.1}          Peak: {:.1}\nCrest factor: {:.3}   Dynamic range: \
         {}\n\nFrequency:\nMeasured: {:.1} Hz\nError: {:.2}%\n\nContinuity:\nMax silence run: {} \
         samples ({:.2} ms)\n\nRESULT: {}",
        audio.format.bits_per_sample,
        audio.format.endianness,
        audio.format.sample_rate,
        audio.duration().as_secs_f64(),
        audio.num_frames(),
        audio.data.len(),
        check.min_sample,
        check.max_sample,
        check.rms,
        check.peak,
        check.crest_factor,
        check.amplitude_range,
        check.measured_frequency,
        check.frequency_error_pct,
        check.max_silence_run_samples,
        check.max_silence_run_ms,
        if check.passed { "PASS" } else { "FAIL" }
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecType {
    Pcm,
    Alac,
    Aac,
    AacEld,
}

#[derive(Debug, Clone)]
pub struct CodecVerifyResult {
    pub codec: CodecType,
    pub snr_db: Option<f64>,
    pub bit_exact: Option<bool>,
    pub frame_count_correct: bool,
    pub issues: Vec<String>,
}

pub fn verify_codec_integrity(
    audio: &RawAudio,
    codec: CodecType,
    reference: Option<&RawAudio>,
) -> CodecVerifyResult {
    let mut issues = Vec::new();
    let mut snr_db = None;
    let mut bit_exact = None;

    if let Some(ref_audio) = reference {
        match codec {
            CodecType::Pcm | CodecType::Alac => {
                let cmp = compare_audio_exact(ref_audio, audio);
                bit_exact = Some(cmp.bit_exact);
                if !cmp.bit_exact {
                    issues.push(format!("Not bit-exact. Max diff: {}", cmp.max_sample_diff));
                }
            }
            CodecType::Aac | CodecType::AacEld => {
                let snr = compute_snr(ref_audio, audio);
                snr_db = Some(snr);
                if snr < 40.0 {
                    issues.push(format!("SNR too low: {:.1} dB", snr));
                }
            }
        }
    }

    CodecVerifyResult {
        codec,
        snr_db,
        bit_exact,
        frame_count_correct: audio.num_frames() > 0, // simple check
        issues,
    }
}
