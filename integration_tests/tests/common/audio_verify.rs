use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, reason = "Used in some test modules but not all")]
pub enum Endianness {
    Little,
    Big,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawAudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub endianness: Endianness,
    pub signed: bool,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
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

#[allow(dead_code, reason = "Used in some test modules but not all")]
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct RawAudio {
    pub data: Vec<u8>,
    pub format: RawAudioFormat,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
impl RawAudio {
    pub fn from_file(path: &Path, format: RawAudioFormat) -> Result<Self, AudioError> {
        let data = std::fs::read(path)?;
        Ok(Self { data, format })
    }

    pub fn from_bytes(data: Vec<u8>, format: RawAudioFormat) -> Self {
        Self { data, format }
    }

    pub fn duration(&self) -> Duration {
        let frames = self.num_frames();
        Duration::from_secs_f64(frames as f64 / self.format.sample_rate as f64)
    }

    pub fn num_frames(&self) -> usize {
        let bytes_per_sample = (self.format.bits_per_sample / 8) as usize;
        let bytes_per_frame = bytes_per_sample * self.format.channels as usize;
        if bytes_per_frame == 0 {
            0
        } else {
            self.data.len() / bytes_per_frame
        }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn samples_i16(&self) -> Vec<i16> {
        if self.format.bits_per_sample != 16 {
            return vec![]; // Simplified
        }

        let mut samples = Vec::with_capacity(self.data.len() / 2);
        for chunk in self.data.chunks_exact(2) {
            let sample = match self.format.endianness {
                Endianness::Little => i16::from_le_bytes([chunk[0], chunk[1]]),
                Endianness::Big => i16::from_be_bytes([chunk[0], chunk[1]]),
            };
            samples.push(sample);
        }
        samples
    }

    pub fn samples_f32(&self) -> Vec<f32> {
        let mut samples = Vec::with_capacity(self.num_frames() * self.format.channels as usize);

        if self.format.bits_per_sample == 16 {
            for sample in self.samples_i16() {
                samples.push(sample as f32 / i16::MAX as f32);
            }
        } else if self.format.bits_per_sample == 24 {
            for chunk in self.data.chunks_exact(3) {
                let sample_i32 = match self.format.endianness {
                    Endianness::Little => {
                        let mut b = [0u8; 4];
                        b[1] = chunk[0];
                        b[2] = chunk[1];
                        b[3] = chunk[2];
                        i32::from_le_bytes(b) >> 8
                    }
                    Endianness::Big => {
                        let mut b = [0u8; 4];
                        b[0] = chunk[0];
                        b[1] = chunk[1];
                        b[2] = chunk[2];
                        i32::from_be_bytes(b) >> 8
                    }
                };
                samples.push(sample_i32 as f32 / 8388607.0);
            }
        }

        samples
    }

    pub fn channel(&self, ch: usize) -> Vec<f32> {
        let all_samples = self.samples_f32();
        let channels = self.format.channels as usize;
        if ch >= channels {
            return vec![];
        }

        all_samples.into_iter().skip(ch).step_by(channels).collect()
    }
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
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

#[allow(dead_code, reason = "Used in some test modules but not all")]
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

#[allow(dead_code, reason = "Used in some test modules but not all")]
#[derive(Debug, thiserror::Error)]
pub enum AudioVerifyError {
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
impl SineWaveCheck {
    pub fn verify(&self, audio: &RawAudio) -> Result<SineWaveResult, AudioVerifyError> {
        let ch = self.channel.unwrap_or(0);

        let i16_samples = if audio.format.bits_per_sample == 16 {
            let all = audio.samples_i16();
            let channels = audio.format.channels as usize;
            all.into_iter()
                .skip(ch)
                .step_by(channels)
                .collect::<Vec<_>>()
        } else {
            // for non-16-bit, do a quick f32 -> i16 conversion for analysis
            audio
                .channel(ch)
                .into_iter()
                .map(|s| (s * i16::MAX as f32) as i16)
                .collect::<Vec<_>>()
        };

        if i16_samples.is_empty() {
            return Err(AudioVerifyError::VerificationFailed(
                "No samples found".into(),
            ));
        }

        let mut min_sample = i16::MAX;
        let mut max_sample = i16::MIN;
        let mut zero_crossings = 0;
        let mut prev_sample = 0i16;

        // Remove DC offset
        let sum: i64 = i16_samples.iter().map(|&s| s as i64).sum();
        let mean = (sum / i16_samples.len() as i64) as i16;

        // Skip first 200ms
        let skip_samples = (audio.format.sample_rate as f32 * 0.2) as usize;
        let analyze_samples = if i16_samples.len() > skip_samples {
            &i16_samples[skip_samples..]
        } else {
            return Err(AudioVerifyError::VerificationFailed(
                "Audio too short after skipping prefix".into(),
            ));
        };

        if analyze_samples.is_empty() {
            return Err(AudioVerifyError::VerificationFailed(
                "Audio too short after skipping prefix".into(),
            ));
        }

        // Trim trailing silence
        let mut end_idx = analyze_samples.len();
        while end_idx > 0 && analyze_samples[end_idx - 1] > -100 && analyze_samples[end_idx - 1] < 100 {
            end_idx -= 1;
        }

        let final_samples = &analyze_samples[..end_idx];
        if final_samples.is_empty() {
            return Err(AudioVerifyError::VerificationFailed(
                "Audio is mostly silence".into(),
            ));
        }

        let mut max_silence_run = 0;
        let mut current_silence_run = 0;
        let mut sq_sum = 0.0f32;
        let mut peak_val = 0.0f32;

        for &raw_sample in final_samples {
            let sample = (raw_sample as i32 - mean as i32) as i16;

            min_sample = min_sample.min(sample);
            max_sample = max_sample.max(sample);

            let s_f32 = sample as f32;
            sq_sum += s_f32 * s_f32;
            peak_val = peak_val.max(s_f32.abs());

            if (prev_sample < 0 && sample >= 0) || (prev_sample >= 0 && sample < 0) {
                zero_crossings += 1;
            }
            prev_sample = sample;

            // Avoid `.abs()` on i16 to prevent panic on i16::MIN
            let is_silence = sample > -100 && sample < 100;
            if is_silence {
                current_silence_run += 1;
                max_silence_run = max_silence_run.max(current_silence_run);
            } else {
                current_silence_run = 0;
            }
        }

        let num_samples = final_samples.len();
        let duration_s = num_samples as f32 / audio.format.sample_rate as f32;

        let rms = (sq_sum / num_samples as f32).sqrt();
        let crest_factor = if rms > 0.0 { peak_val / rms } else { 0.0 };

        let estimated_frequency_zc = (zero_crossings as f32 / duration_s) / 2.0;

        let measured_frequency = estimated_frequency_zc; // using zero-crossing for now

        let frequency_error_pct = if self.expected_frequency > 0.0 {
            ((measured_frequency - self.expected_frequency).abs() / self.expected_frequency) * 100.0
        } else {
            0.0
        };

        let amplitude_range = (max_sample as i32) - (min_sample as i32);
        let max_silence_run_ms =
            (max_silence_run as f32 / audio.format.sample_rate as f32) * 1000.0;

        let mut passed = true;
        let mut failure_reasons = Vec::new();

        if self.check_amplitude && amplitude_range < self.min_amplitude as i32 {
            passed = false;
            failure_reasons.push(format!("Amplitude range too low: {}", amplitude_range));
        }

        if self.check_frequency && frequency_error_pct > self.frequency_tolerance_pct {
            passed = false;
            failure_reasons.push(format!(
                "Frequency error too high: {:.2}%",
                frequency_error_pct
            ));
        }

        if self.check_continuity && max_silence_run_ms > self.max_silence_run_ms {
            passed = false;
            failure_reasons.push(format!("Silence run too long: {:.2}ms", max_silence_run_ms));
        }

        Ok(SineWaveResult {
            measured_frequency,
            frequency_error_pct,
            min_sample,
            max_sample,
            amplitude_range,
            rms,
            peak: peak_val,
            crest_factor,
            max_silence_run_samples: max_silence_run,
            max_silence_run_ms,
            num_frames: audio.num_frames(),
            duration: audio.duration(),
            passed,
            failure_reasons,
        })
    }
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
impl SineWaveResult {
    pub fn assert_passed(&self) -> Result<(), AudioVerifyError> {
        if self.passed {
            Ok(())
        } else {
            Err(AudioVerifyError::VerificationFailed(
                self.failure_reasons.join(", "),
            ))
        }
    }
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct StereoSineCheck {
    pub left_frequency: f32,
    pub right_frequency: f32,
    pub frequency_tolerance_pct: f32,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct StereoSineResult {
    pub left: SineWaveResult,
    pub right: SineWaveResult,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
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

        Ok(StereoSineResult {
            left: left_check.verify(audio)?,
            right: right_check.verify(audio)?,
        })
    }
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
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

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub fn compare_audio_exact(sent: &RawAudio, received: &RawAudio) -> CompareResult {
    let sent_samples = sent.samples_i16();
    let recv_samples = received.samples_i16();

    let (offset, _) = align_audio_i16(
        &sent_samples,
        &recv_samples,
        sent.format.sample_rate as usize * 2,
    );

    let sent_frames = sent.num_frames();
    let received_frames = received.num_frames();

    let mut matching_frames = 0;
    let mut first_mismatch_frame = None;
    let mut max_sample_diff = 0;
    let mut sum_diff = 0.0;

    let channels = sent.format.channels as usize;
    let compare_len = std::cmp::min(
        sent_samples.len(),
        recv_samples.len().saturating_sub(offset),
    );

    for i in 0..compare_len {
        let diff = (sent_samples[i] as i32 - recv_samples[i + offset] as i32).abs();
        max_sample_diff = max_sample_diff.max(diff);
        sum_diff += diff as f64;

        if diff == 0 {
            if i % channels == 0 {
                matching_frames += 1;
            }
        } else if first_mismatch_frame.is_none() {
            first_mismatch_frame = Some(i / channels);
        }
    }

    let mean_sample_diff = if compare_len > 0 {
        sum_diff / compare_len as f64
    } else {
        0.0
    };

    CompareResult {
        sample_count_match: (sent_frames as i64 - received_frames as i64).abs() <= 1,
        sent_frames,
        received_frames,
        matching_frames,
        first_mismatch_frame,
        max_sample_diff,
        mean_sample_diff,
        bit_exact: max_sample_diff == 0 && compare_len > 0,
    }
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub fn compute_snr(original: &RawAudio, received: &RawAudio) -> f64 {
    let orig_samples = original.samples_f32();
    let recv_samples = received.samples_f32();

    let (offset, _) = align_audio(
        &orig_samples,
        &recv_samples,
        original.format.sample_rate as usize * 2,
    );

    let len = std::cmp::min(
        orig_samples.len(),
        recv_samples.len().saturating_sub(offset),
    );
    if len == 0 {
        return 0.0;
    }

    let mut signal_power = 0.0f64;
    let mut noise_power = 0.0f64;

    for i in 0..len {
        let orig = orig_samples[i] as f64;
        let recv = recv_samples[i + offset] as f64;
        let diff = orig - recv;

        signal_power += orig * orig;
        noise_power += diff * diff;
    }

    if noise_power < 1e-10 {
        return 100.0; // Perfect match or very low noise
    }

    10.0 * (signal_power / noise_power).log10()
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub fn align_audio_i16(reference: &[i16], captured: &[i16], max_offset: usize) -> (usize, f64) {
    if reference.is_empty() || captured.is_empty() {
        return (0, 0.0);
    }

    let mut best_offset = 0;
    let mut best_corr = 0.0;

    let corr_len = std::cmp::min(reference.len(), 1000);
    let mut ref_energy = 0.0;
    for &s in reference.iter().take(corr_len) {
        let f = s as f64;
        ref_energy += f * f;
    }

    if ref_energy == 0.0 {
        return (0, 0.0);
    }

    for offset in 0..max_offset.min(captured.len().saturating_sub(corr_len)) {
        let mut dot = 0.0;
        let mut cap_energy = 0.0;

        for i in 0..corr_len {
            let rf = reference[i] as f64;
            let cf = captured[i + offset] as f64;
            dot += rf * cf;
            cap_energy += cf * cf;
        }

        if cap_energy > 0.0 {
            let corr = dot / (ref_energy.sqrt() * cap_energy.sqrt());
            if corr > best_corr {
                best_corr = corr;
                best_offset = offset;
            }
        }
    }

    (best_offset, best_corr)
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub fn align_audio(reference: &[f32], captured: &[f32], max_offset: usize) -> (usize, f64) {
    if reference.is_empty() || captured.is_empty() {
        return (0, 0.0);
    }

    let mut offset = 0;
    for (i, &s) in captured.iter().enumerate().take(max_offset) {
        if s.abs() > 0.003 {
            // ~100/32768
            offset = i;
            break;
        }
    }

    (offset, 1.0)
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub fn measure_onset_latency(audio: &RawAudio, threshold: f32) -> Duration {
    let samples = audio.samples_f32();
    for (i, &s) in samples.iter().enumerate() {
        if s.abs() >= threshold {
            return Duration::from_secs_f64(
                i as f64 / (audio.format.sample_rate as f64 * audio.format.channels as f64),
            );
        }
    }
    Duration::ZERO
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
#[derive(Debug, Clone)]
pub struct GapInfo {
    pub start_frame: usize,
    pub end_frame: usize,
    pub duration: Duration,
    pub position: Duration,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub fn measure_gap_latency(audio: &RawAudio, gap_threshold_ms: f32) -> Vec<GapInfo> {
    let mut gaps = Vec::new();
    let samples = audio.samples_i16();
    let channels = audio.format.channels as usize;
    let sample_rate = audio.format.sample_rate as f32;

    let mut silence_start = None;

    for (i, chunk) in samples.chunks_exact(channels).enumerate() {
        let is_silent = chunk.iter().all(|&s| s.abs() < 100);

        if is_silent {
            if silence_start.is_none() {
                silence_start = Some(i);
            }
        } else if let Some(start) = silence_start {
            let duration_frames = i - start;
            let duration_ms = (duration_frames as f32 / sample_rate) * 1000.0;

            if duration_ms >= gap_threshold_ms {
                gaps.push(GapInfo {
                    start_frame: start,
                    end_frame: i,
                    duration: Duration::from_secs_f64(duration_frames as f64 / sample_rate as f64),
                    position: Duration::from_secs_f64(start as f64 / sample_rate as f64),
                });
            }
            silence_start = None;
        }
    }

    gaps
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecType {
    Pcm,
    Alac,
    Aac,
    AacEld,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct CodecVerifyResult {
    pub codec: CodecType,
    pub snr_db: Option<f64>,
    pub bit_exact: Option<bool>,
    pub frame_count_correct: bool,
    pub issues: Vec<String>,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
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
        match codec {
            CodecType::Pcm | CodecType::Alac => {
                let cmp = compare_audio_exact(ref_audio, audio);
                bit_exact = Some(cmp.bit_exact);
                frame_count_correct = cmp.sample_count_match;
                if !cmp.bit_exact {
                    issues.push("Not bit exact".to_string());
                }
            }
            CodecType::Aac | CodecType::AacEld => {
                let snr = compute_snr(ref_audio, audio);
                snr_db = Some(snr);
                if snr < 40.0 {
                    issues.push(format!("Low SNR: {:.2} dB", snr));
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

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub trait AudioCheck {
    fn report(&self) -> String;
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub fn audio_diagnostic_report(audio: &RawAudio, _checks: &[Box<dyn AudioCheck>]) -> String {
    let check = SineWaveCheck::default();
    let res = check.verify(audio).unwrap_or(SineWaveResult {
        measured_frequency: 0.0,
        frequency_error_pct: 0.0,
        min_sample: 0,
        max_sample: 0,
        amplitude_range: 0,
        rms: 0.0,
        peak: 0.0,
        crest_factor: 0.0,
        max_silence_run_samples: 0,
        max_silence_run_ms: 0.0,
        num_frames: audio.num_frames(),
        duration: audio.duration(),
        passed: false,
        failure_reasons: vec!["Verification failed".into()],
    });

    format!(
        "Audio Diagnostic Report\n=======================\nFormat: {}-bit {:?} {} ch @ {} \
         Hz\nDuration: {:.2}s ({} frames)\nData size: {} bytes\n\nAmplitude:\n  Min sample: {}    \
         Max sample: {}\n  RMS: {:.1}          Peak: {:.1}\n  Crest factor: {:.3}   Dynamic \
         range: {}\n\nFrequency (left channel):\n  Zero-crossing estimate: {:.1} Hz\n  Expected: \
         {:.1} Hz    Error: {:.2}%\n\nContinuity:\n  Max silence run: {} samples ({:.2} \
         ms)\n\nRESULT: {}",
        audio.format.bits_per_sample,
        audio.format.endianness,
        audio.format.channels,
        audio.format.sample_rate,
        res.duration.as_secs_f64(),
        res.num_frames,
        audio.data.len(),
        res.min_sample,
        res.max_sample,
        res.rms,
        res.peak,
        res.crest_factor,
        res.amplitude_range,
        res.measured_frequency,
        check.expected_frequency,
        res.frequency_error_pct,
        res.max_silence_run_samples,
        res.max_silence_run_ms,
        if res.passed { "PASS" } else { "FAIL" }
    )
}
