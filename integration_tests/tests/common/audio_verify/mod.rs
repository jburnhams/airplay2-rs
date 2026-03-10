use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "Used in format specification even if not all variants hit"
)]
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
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub endianness: Endianness,
    pub signed: bool,
}

#[derive(Debug)]
#[allow(dead_code, reason = "Part of verification API")]
pub enum AudioError {
    IoError(std::io::Error),
    InvalidFormat(String),
}

impl From<std::io::Error> for AudioError {
    fn from(err: std::io::Error) -> Self {
        AudioError::IoError(err)
    }
}

impl RawAudio {
    #[allow(dead_code, reason = "Utility API for verification from files")]
    pub fn from_file(path: &Path, format: RawAudioFormat) -> Result<Self, AudioError> {
        let data = fs::read(path)?;
        Ok(Self::from_bytes(data, format))
    }

    pub fn from_bytes(data: Vec<u8>, format: RawAudioFormat) -> Self {
        Self {
            data,
            sample_rate: format.sample_rate,
            channels: format.channels,
            bits_per_sample: format.bits_per_sample,
            endianness: format.endianness,
            signed: format.signed,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn num_frames(&self) -> usize {
        let bytes_per_sample = (self.bits_per_sample / 8) as usize;
        let bytes_per_frame = bytes_per_sample * self.channels as usize;
        if bytes_per_frame == 0 {
            0
        } else {
            self.data.len() / bytes_per_frame
        }
    }

    pub fn duration(&self) -> Duration {
        let frames = self.num_frames();
        if self.sample_rate == 0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(frames as f64 / self.sample_rate as f64)
        }
    }

    pub fn samples_i16(&self) -> Vec<i16> {
        let mut samples = Vec::with_capacity(self.num_frames() * self.channels as usize);
        let bytes_per_sample = (self.bits_per_sample / 8) as usize;

        for chunk in self.data.chunks_exact(bytes_per_sample) {
            match (self.bits_per_sample, self.endianness, self.signed) {
                (16, Endianness::Little, true) => {
                    samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
                }
                (16, Endianness::Big, true) => {
                    samples.push(i16::from_be_bytes([chunk[0], chunk[1]]));
                }
                (24, Endianness::Little, true) => {
                    // To properly sign extend a 24-bit value in little-endian format:
                    // Pad with 0 at the least significant byte to get 32 bits,
                    // shifting the 24 bits to the top of the i32 to maintain sign bit position,
                    // then arithmetic right shift by 16 to get the top 16 bits sign-extended
                    // correctly.
                    let mut extended = i32::from_le_bytes([0, chunk[0], chunk[1], chunk[2]]);
                    extended >>= 8; // First sign extend from 24 to 32 bit
                    let final_val = extended >> 8; // Now take top 16 bits
                    samples.push(final_val as i16);
                }
                (24, Endianness::Big, true) => {
                    let mut extended = i32::from_be_bytes([0, chunk[0], chunk[1], chunk[2]]);
                    extended <<= 8;
                    extended >>= 16;
                    samples.push(extended as i16);
                }
                _ => {
                    // Unsupported format fallback for testing
                    samples.push(0);
                }
            }
        }
        samples
    }

    pub fn samples_f32(&self) -> Vec<f32> {
        self.samples_i16()
            .into_iter()
            .map(|s| s as f32 / i16::MAX as f32)
            .collect()
    }

    pub fn channel(&self, ch: usize) -> Vec<f32> {
        let all_samples = self.samples_f32();
        let channels = self.channels as usize;
        all_samples
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % channels == ch)
            .map(|(_, s)| s)
            .collect()
    }
}

#[derive(Debug)]
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
#[allow(dead_code, reason = "Data struct exposed for detailed test assertions")]
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

#[derive(Debug)]
pub enum AudioVerifyError {
    CheckFailed(SineWaveResult),
    InsufficientData(String),
}

impl std::fmt::Display for AudioVerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioVerifyError::CheckFailed(res) => {
                write!(f, "Audio verification failed: {:?}", res.failure_reasons)
            }
            AudioVerifyError::InsufficientData(msg) => write!(f, "Insufficient data: {}", msg),
        }
    }
}

impl std::error::Error for AudioVerifyError {}

impl SineWaveResult {
    pub fn assert_passed(&self) -> Result<(), AudioVerifyError> {
        if self.passed {
            Ok(())
        } else {
            Err(AudioVerifyError::CheckFailed(self.clone()))
        }
    }
}

impl SineWaveCheck {
    pub fn verify(&self, audio: &RawAudio) -> Result<SineWaveResult, AudioVerifyError> {
        if audio.is_empty() {
            return Err(AudioVerifyError::InsufficientData("Audio is empty".into()));
        }

        let num_frames = audio.num_frames();
        let duration = audio.duration();
        let sample_rate = audio.sample_rate as f32;

        let samples_f32 = match self.channel {
            Some(ch) => audio.channel(ch),
            None => audio.channel(0), // default left channel
        };

        if samples_f32.len() < 1000 {
            // Need enough samples for reasonable estimation
            return Err(AudioVerifyError::InsufficientData(
                "Not enough samples for frequency analysis".into(),
            ));
        }

        // DC Offset removal
        let mean = samples_f32.iter().sum::<f32>() / samples_f32.len() as f32;
        let centered_samples: Vec<f32> = samples_f32.iter().map(|&s| s - mean).collect();

        // Amplitude stats
        let mut min_val = 1.0f32;
        let mut max_val = -1.0f32;
        let mut sum_sq = 0.0;
        let mut max_silence_run = 0;
        let mut current_silence_run = 0;
        let silence_threshold = 0.003f32; // ~100 / 32768

        let mut zero_crossings = 0;
        let mut prev_sample = centered_samples[0];

        // Skip the first N ms to avoid onset latency/artifacts if possible
        let skip_samples = (sample_rate * 0.2) as usize; // 200ms
        let start_idx = skip_samples.min(centered_samples.len() / 4);

        let analysis_samples = &centered_samples[start_idx..];
        let analysis_duration = analysis_samples.len() as f32 / sample_rate;

        for &sample in analysis_samples {
            min_val = min_val.min(sample);
            max_val = max_val.max(sample);
            sum_sq += sample * sample;

            if sample.abs() < silence_threshold {
                current_silence_run += 1;
                max_silence_run = max_silence_run.max(current_silence_run);
            } else {
                current_silence_run = 0;
            }

            if (prev_sample < 0.0 && sample >= 0.0) || (prev_sample >= 0.0 && sample < 0.0) {
                zero_crossings += 1;
            }
            prev_sample = sample;
        }

        let rms = (sum_sq / analysis_samples.len() as f32).sqrt() * i16::MAX as f32;
        let peak = max_val.abs().max(min_val.abs()) * i16::MAX as f32;
        let crest_factor = if rms > 0.0 { peak / rms } else { 0.0 };

        let min_sample = (min_val * i16::MAX as f32) as i16;
        let max_sample = (max_val * i16::MAX as f32) as i16;
        let amplitude_range = (max_sample as i32) - (min_sample as i32);

        // Frequency estimation
        let measured_frequency = (zero_crossings as f32 / analysis_duration) / 2.0;
        let frequency_error_pct = if self.expected_frequency > 0.0 {
            ((measured_frequency - self.expected_frequency).abs() / self.expected_frequency) * 100.0
        } else {
            0.0
        };

        let max_silence_run_ms = (max_silence_run as f32 / sample_rate) * 1000.0;

        let mut failure_reasons = Vec::new();

        if self.check_amplitude && amplitude_range < self.min_amplitude as i32 {
            failure_reasons.push(format!(
                "Amplitude too low: range {} < min {}",
                amplitude_range, self.min_amplitude
            ));
        }

        if self.check_frequency && frequency_error_pct > self.frequency_tolerance_pct {
            failure_reasons.push(format!(
                "Frequency mismatch: expected {}Hz, measured {:.1}Hz (error: {:.1}% > {:.1}%)",
                self.expected_frequency,
                measured_frequency,
                frequency_error_pct,
                self.frequency_tolerance_pct
            ));
        }

        if self.check_continuity && max_silence_run_ms > self.max_silence_run_ms {
            failure_reasons.push(format!(
                "Suspicious silence: {:.1}ms > max {:.1}ms",
                max_silence_run_ms, self.max_silence_run_ms
            ));
        }

        let passed = failure_reasons.is_empty();

        Ok(SineWaveResult {
            measured_frequency,
            frequency_error_pct,
            min_sample,
            max_sample,
            amplitude_range,
            rms,
            peak,
            crest_factor,
            max_silence_run_samples: max_silence_run,
            max_silence_run_ms,
            num_frames,
            duration,
            passed,
            failure_reasons,
        })
    }
}

#[derive(Debug)]
pub struct StereoSineCheck {
    pub left_frequency: f32,
    pub right_frequency: f32,
    pub frequency_tolerance_pct: f32,
}

#[derive(Debug, Clone)]
pub struct StereoSineResult {
    pub left: SineWaveResult,
    pub right: SineWaveResult,
}

impl StereoSineCheck {
    pub fn verify(&self, audio: &RawAudio) -> Result<StereoSineResult, AudioVerifyError> {
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
#[allow(dead_code, reason = "Data struct exposed for detailed test assertions")]
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

pub fn align_audio(reference: &[f32], captured: &[f32], max_offset: usize) -> (usize, f64) {
    if reference.is_empty() || captured.is_empty() {
        return (0, 0.0);
    }

    let search_window = max_offset.min(captured.len());
    let mut best_offset = 0;
    let mut max_corr = -1.0;

    let corr_len = reference.len().min(captured.len() - search_window);
    if corr_len == 0 {
        return (0, 0.0);
    }

    // Reference energy
    let ref_energy: f32 = reference[..corr_len].iter().map(|x| x * x).sum();

    for offset in 0..search_window {
        let mut corr = 0.0;
        let mut cap_energy = 0.0;
        for i in 0..corr_len {
            corr += reference[i] * captured[i + offset];
            cap_energy += captured[i + offset] * captured[i + offset];
        }

        let normalized_corr = if ref_energy > 0.0 && cap_energy > 0.0 {
            corr / (ref_energy * cap_energy).sqrt()
        } else {
            0.0
        };

        if normalized_corr > max_corr {
            max_corr = normalized_corr;
            best_offset = offset;
        }
    }

    (best_offset, max_corr as f64)
}

pub fn compare_audio_exact(sent: &RawAudio, received: &RawAudio) -> CompareResult {
    let sent_samples = sent.samples_i16();
    let recv_samples = received.samples_i16();

    let sent_frames = sent.num_frames();
    let received_frames = received.num_frames();

    // Convert to f32 for alignment
    let sent_f32 = sent.samples_f32();
    let recv_f32 = received.samples_f32();

    let max_offset = (received.sample_rate * 2) as usize * received.channels as usize; // 2 seconds max search
    let (offset_samples, _) = align_audio(&sent_f32, &recv_f32, max_offset);

    // Ensure the offset corresponds to a full frame boundary
    let channels = sent.channels as usize;
    let offset_frames = (offset_samples as f64 / channels as f64).round() as usize;
    let align_offset_samples = offset_frames * channels;

    let mut matching_frames = 0;
    let mut first_mismatch = None;
    let mut max_diff = 0;
    let mut sum_diff = 0.0;

    let compare_len =
        sent_frames.min((recv_samples.len().saturating_sub(align_offset_samples)) / channels);

    for frame in 0..compare_len {
        let mut frame_match = true;
        for ch in 0..channels {
            let s_idx = frame * channels + ch;
            let r_idx = align_offset_samples + frame * channels + ch;

            if s_idx < sent_samples.len() && r_idx < recv_samples.len() {
                let s = sent_samples[s_idx];
                let r = recv_samples[r_idx];
                let diff = (s as i32 - r as i32).abs();

                if diff > 0 {
                    frame_match = false;
                    max_diff = max_diff.max(diff);
                    if first_mismatch.is_none() {
                        first_mismatch = Some(frame);
                    }
                }
                sum_diff += diff as f64;
            }
        }
        if frame_match {
            matching_frames += 1;
        }
    }

    CompareResult {
        sample_count_match: sent_frames.abs_diff(received_frames) <= 1,
        sent_frames,
        received_frames,
        matching_frames,
        first_mismatch_frame: first_mismatch,
        max_sample_diff: max_diff,
        mean_sample_diff: if compare_len > 0 {
            sum_diff / (compare_len * channels) as f64
        } else {
            0.0
        },
        bit_exact: first_mismatch.is_none() && compare_len > 0,
    }
}

pub fn compute_snr(original: &RawAudio, received: &RawAudio) -> f64 {
    let orig_f32 = original.samples_f32();
    let recv_f32 = received.samples_f32();

    let max_offset = (received.sample_rate * 2) as usize * received.channels as usize;
    let (offset, _) = align_audio(&orig_f32, &recv_f32, max_offset);

    let mut signal_energy = 0.0;
    let mut noise_energy = 0.0;

    let len = orig_f32.len().min(recv_f32.len().saturating_sub(offset));
    if len == 0 {
        return 0.0;
    }

    for i in 0..len {
        let s = orig_f32[i];
        let r = recv_f32[i + offset];
        signal_energy += s * s;
        let noise = s - r;
        noise_energy += noise * noise;
    }

    if noise_energy == 0.0 {
        return f64::INFINITY;
    }

    10.0 * (signal_energy / noise_energy).log10() as f64
}

pub fn measure_onset_latency(audio: &RawAudio, threshold: f32) -> Duration {
    let samples = audio.samples_f32();
    let sample_rate = audio.sample_rate as f32;
    let channels = audio.channels as usize;

    for (i, &s) in samples.iter().enumerate() {
        if s.abs() >= threshold {
            let frame = i / channels;
            return Duration::from_secs_f64(frame as f64 / sample_rate as f64);
        }
    }
    audio.duration() // Max latency
}

#[derive(Debug, Clone)]
#[allow(dead_code, reason = "Data struct exposed for detailed test assertions")]
pub struct GapInfo {
    pub start_frame: usize,
    pub end_frame: usize,
    pub duration: Duration,
    pub position: Duration,
}

pub fn measure_gap_latency(audio: &RawAudio, gap_threshold_ms: f32) -> Vec<GapInfo> {
    let samples = audio.samples_f32();
    let sample_rate = audio.sample_rate as f32;
    let channels = audio.channels as usize;
    let mut gaps = Vec::new();

    let silence_threshold = 0.003f32;
    let mut in_gap = false;
    let mut gap_start_frame = 0;

    // Group into frames to avoid channel interleaving issues
    let num_frames = samples.len() / channels;
    for frame in 0..num_frames {
        let mut is_silent = true;
        for ch in 0..channels {
            if samples[frame * channels + ch].abs() >= silence_threshold {
                is_silent = false;
                break;
            }
        }

        if is_silent {
            if !in_gap {
                in_gap = true;
                gap_start_frame = frame;
            }
        } else if in_gap {
            in_gap = false;
            let gap_frames = frame - gap_start_frame;
            let gap_duration_ms = (gap_frames as f32 / sample_rate) * 1000.0;

            if gap_duration_ms >= gap_threshold_ms {
                gaps.push(GapInfo {
                    start_frame: gap_start_frame,
                    end_frame: frame,
                    duration: Duration::from_secs_f64(gap_frames as f64 / sample_rate as f64),
                    position: Duration::from_secs_f64(gap_start_frame as f64 / sample_rate as f64),
                });
            }
        }
    }

    // Handle gap at end
    if in_gap {
        let gap_frames = num_frames - gap_start_frame;
        let gap_duration_ms = (gap_frames as f32 / sample_rate) * 1000.0;
        if gap_duration_ms >= gap_threshold_ms {
            gaps.push(GapInfo {
                start_frame: gap_start_frame,
                end_frame: num_frames,
                duration: Duration::from_secs_f64(gap_frames as f64 / sample_rate as f64),
                position: Duration::from_secs_f64(gap_start_frame as f64 / sample_rate as f64),
            });
        }
    }

    gaps
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, reason = "Part of verification API")]
pub enum CodecType {
    Pcm,
    Alac,
    Aac,
    AacEld,
}

#[derive(Debug, Clone)]
#[allow(dead_code, reason = "Part of verification API")]
pub struct CodecVerifyResult {
    pub codec: CodecType,
    pub snr_db: Option<f64>,
    pub bit_exact: Option<bool>,
    pub frame_count_correct: bool,
    pub issues: Vec<String>,
}

#[allow(dead_code, reason = "Part of verification API")]
pub fn verify_codec_integrity(
    audio: &RawAudio,
    codec: CodecType,
    reference: Option<&RawAudio>,
) -> CodecVerifyResult {
    let mut issues = Vec::new();
    let mut snr_db = None;
    let mut bit_exact = None;
    let mut frame_count_correct = true;

    if let Some(ref_audio) = reference {
        let comp = compare_audio_exact(ref_audio, audio);

        if !comp.sample_count_match {
            frame_count_correct = false;
            issues.push(format!(
                "Frame count mismatch: {} != {}",
                comp.received_frames, comp.sent_frames
            ));
        }

        match codec {
            CodecType::Pcm | CodecType::Alac => {
                bit_exact = Some(comp.bit_exact);
                if !comp.bit_exact {
                    issues.push("Audio is not bit-exact".into());
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
        frame_count_correct,
        issues,
    }
}

#[allow(dead_code, reason = "Part of verification API")]
pub trait AudioCheck {
    fn check(&self, audio: &RawAudio) -> String;
}

#[cfg(test)]
mod tests;

pub fn audio_diagnostic_report(audio: &RawAudio, filename: &str) -> String {
    let mut report = String::new();
    report.push_str("Audio Diagnostic Report\n");
    report.push_str("=======================\n");
    report.push_str(&format!("File: {}\n", filename));
    report.push_str(&format!(
        "Format: {}-bit {:?} {} @ {} Hz\n",
        audio.bits_per_sample,
        audio.endianness,
        if audio.channels == 2 {
            "stereo"
        } else {
            "mono"
        },
        audio.sample_rate
    ));

    let dur = audio.duration().as_secs_f64();
    report.push_str(&format!(
        "Duration: {:.2}s ({} frames)\n",
        dur,
        audio.num_frames()
    ));
    report.push_str(&format!("Data size: {} bytes\n\n", audio.data.len()));

    if let Ok(result) = SineWaveCheck::default().verify(audio) {
        report.push_str("Amplitude:\n");
        report.push_str(&format!(
            "  Min sample: {:<8} Max sample: {}\n",
            result.min_sample, result.max_sample
        ));
        report.push_str(&format!(
            "  RMS: {:.1}          Peak: {:.1}\n",
            result.rms, result.peak
        ));
        report.push_str(&format!(
            "  Crest factor: {:.3}   Dynamic range: {}\n\n",
            result.crest_factor, result.amplitude_range
        ));

        report.push_str("Frequency (left channel):\n");
        report.push_str(&format!(
            "  Estimated: {:.1} Hz\n",
            result.measured_frequency
        ));

        report.push_str("\nContinuity:\n");
        report.push_str(&format!(
            "  Max silence run: {} samples ({:.2} ms)\n",
            result.max_silence_run_samples, result.max_silence_run_ms
        ));
    }

    report
}
