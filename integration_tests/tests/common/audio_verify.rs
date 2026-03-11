use std::path::Path;
use std::time::Duration;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
    pub fn assert_passed(&self) -> Result<(), AudioVerifyError> {
        if self.passed {
            Ok(())
        } else {
            Err(AudioVerifyError::VerificationFailed(
                self.failure_reasons.join("\n"),
            ))
        }
    }
}

impl SineWaveCheck {
    pub fn new(expected_frequency: f32) -> Self {
        Self {
            expected_frequency,
            ..Default::default()
        }
    }

    pub fn verify(&self, audio: &RawAudio) -> Result<SineWaveResult, AudioVerifyError> {
        if audio.is_empty() {
            return Err(AudioVerifyError::VerificationFailed("Audio is empty".into()));
        }

        // Apply skip first N ms of audio (setup latency skip)
        let setup_latency_ms = 200.0;
        let setup_latency_samples = (audio.sample_rate as f32 * (setup_latency_ms / 1000.0)) as usize;
        let mut num_frames = audio.num_frames();

        let start_frame = setup_latency_samples.min(num_frames);

        // Channel selection
        let all_samples = match self.channel {
            Some(ch) => audio.channel(ch),
            None => audio.channel(0),
        };

        let mut samples = all_samples.into_iter().skip(start_frame).collect::<Vec<_>>();
        num_frames = samples.len();

        if num_frames == 0 {
            return Err(AudioVerifyError::VerificationFailed(
                "Audio is too short after skipping setup latency".into(),
            ));
        }

        // Truncate trailing zeros
        while let Some(&last) = samples.last() {
            if last.abs() < 1e-4 {
                samples.pop();
            } else {
                break;
            }
        }
        num_frames = samples.len();

        if num_frames == 0 {
            return Err(AudioVerifyError::VerificationFailed(
                "Audio is entirely silence after setup latency".into(),
            ));
        }

        let duration = Duration::from_secs_f64(num_frames as f64 / audio.sample_rate as f64);
        let duration_secs = duration.as_secs_f32();

        // Calculate basic statistics
        let mut sum_sq = 0.0;
        let mut peak_val: f32 = 0.0;
        let mut min_sample_val: f32 = 1.0;
        let mut max_sample_val: f32 = -1.0;

        // Remove DC offset
        let sum: f32 = samples.iter().sum();
        let mean = sum / num_frames as f32;

        for sample in samples.iter_mut() {
            *sample -= mean;

            let abs_val = sample.abs();
            if abs_val > peak_val {
                peak_val = abs_val;
            }
            if *sample < min_sample_val {
                min_sample_val = *sample;
            }
            if *sample > max_sample_val {
                max_sample_val = *sample;
            }

            sum_sq += *sample * *sample;
        }

        let rms = (sum_sq / num_frames as f32).sqrt();
        let crest_factor = if rms > 0.0 { peak_val / rms } else { 0.0 };

        // Scale to i16 for some of the output stats
        let min_sample = (min_sample_val * 32768.0) as i16;
        let max_sample = (max_sample_val * 32768.0) as i16;
        let amplitude_range = (max_sample as i32) - (min_sample as i32);

        // Frequency estimation
        let mut zero_crossings = 0;
        for i in 1..num_frames {
            if (samples[i - 1] < 0.0 && samples[i] >= 0.0)
                || (samples[i - 1] >= 0.0 && samples[i] < 0.0)
            {
                zero_crossings += 1;
            }
        }

        let zc_frequency = if duration_secs > 0.0 {
            (zero_crossings as f32 / duration_secs) / 2.0
        } else {
            0.0
        };

        // Autocorrelation frequency estimation
        let min_freq = 20.0;
        let max_lag = (audio.sample_rate as f32 / min_freq).ceil() as usize;
        let max_lag = max_lag.min(num_frames / 2);

        let mut autocorr = vec![0.0; max_lag];
        for lag in 0..max_lag {
            let mut sum = 0.0;
            for i in 0..(num_frames - lag) {
                sum += samples[i] * samples[i + lag];
            }
            autocorr[lag] = sum;
        }

        // Find first peak
        let mut ac_frequency = 0.0;
        let mut found_valley = false;
        let mut peak_lag = 0;
        let mut max_ac = -1.0;

        for lag in 1..max_lag {
            if autocorr[lag] < autocorr[lag - 1] && !found_valley {
                continue;
            }
            if autocorr[lag] > autocorr[lag - 1] {
                found_valley = true;
            }

            if found_valley {
                if autocorr[lag] > max_ac {
                    max_ac = autocorr[lag];
                    peak_lag = lag;
                } else if autocorr[lag] < autocorr[lag - 1] && peak_lag > 0 {
                    // We found a local maximum
                    break;
                }
            }
        }

        if peak_lag > 0 {
            ac_frequency = audio.sample_rate as f32 / peak_lag as f32;
        }

        let mut measured_frequency = zc_frequency;
        let zc_error = (zc_frequency - self.expected_frequency).abs() / self.expected_frequency * 100.0;
        let ac_error = (ac_frequency - self.expected_frequency).abs() / self.expected_frequency * 100.0;

        // Use AC if it's better and ZC is way off, otherwise stick to ZC.
        if zc_error > self.frequency_tolerance_pct && ac_error <= self.frequency_tolerance_pct {
            measured_frequency = ac_frequency;
        }

        let frequency_error_pct = (measured_frequency - self.expected_frequency).abs() / self.expected_frequency * 100.0;

        // Continuity check
        let mut max_silence_run_samples = 0;
        let mut current_silence_run = 0;
        // Near-zero threshold
        let silence_threshold = 100.0 / 32768.0;

        for &sample in &samples {
            if sample.abs() < silence_threshold {
                current_silence_run += 1;
                if current_silence_run > max_silence_run_samples {
                    max_silence_run_samples = current_silence_run;
                }
            } else {
                current_silence_run = 0;
            }
        }

        let max_silence_run_ms = (max_silence_run_samples as f32 / audio.sample_rate as f32) * 1000.0;

        let mut failure_reasons = Vec::new();
        let mut passed = true;

        if self.check_amplitude && amplitude_range < self.min_amplitude as i32 {
            passed = false;
            failure_reasons.push(format!(
                "Amplitude range {} is less than minimum {}",
                amplitude_range, self.min_amplitude
            ));
        }

        if self.check_frequency && duration_secs > 0.5 && frequency_error_pct > self.frequency_tolerance_pct {
            passed = false;
            failure_reasons.push(format!(
                "Frequency error {:.2}% is greater than tolerance {:.2}% (measured: {:.2} Hz, expected: {:.2} Hz)",
                frequency_error_pct, self.frequency_tolerance_pct, measured_frequency, self.expected_frequency
            ));
        } else if self.check_frequency && duration_secs <= 0.5 {
            // Very short audio warning, check skipped or relaxed
            tracing::warn!("Very short audio, frequency estimate might be inaccurate.");
        }

        if self.check_continuity && max_silence_run_ms > self.max_silence_run_ms {
            passed = false;
            failure_reasons.push(format!(
                "Max silence run {:.2} ms is greater than allowed {:.2} ms",
                max_silence_run_ms, self.max_silence_run_ms
            ));
        }

        Ok(SineWaveResult {
            measured_frequency,
            frequency_error_pct,
            min_sample,
            max_sample,
            amplitude_range,
            rms: rms * 32768.0,
            peak: peak_val * 32768.0,
            crest_factor,
            max_silence_run_samples,
            max_silence_run_ms,
            num_frames,
            duration,
            passed,
            failure_reasons,
        })
    }
}

#[derive(Debug, Clone)]
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
        let mut left_check = SineWaveCheck::new(self.left_frequency);
        left_check.frequency_tolerance_pct = self.frequency_tolerance_pct;
        left_check.channel = Some(0);

        let mut right_check = SineWaveCheck::new(self.right_frequency);
        right_check.frequency_tolerance_pct = self.frequency_tolerance_pct;
        right_check.channel = Some(1);

        let left = left_check.verify(audio)?;
        let right = right_check.verify(audio)?;

        Ok(StereoSineResult { left, right })
    }
}

pub fn measure_onset_latency(audio: &RawAudio, threshold: f32) -> Duration {
    let all_samples = audio.samples_f32();
    let num_channels = audio.channels as usize;
    let mut first_frame = 0;

    for (i, chunk) in all_samples.chunks_exact(num_channels).enumerate() {
        if chunk.iter().any(|&s| s.abs() >= threshold) {
            first_frame = i;
            break;
        }
    }

    if audio.sample_rate > 0 {
        Duration::from_secs_f64(first_frame as f64 / audio.sample_rate as f64)
    } else {
        Duration::from_secs(0)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GapInfo {
    pub start_frame: usize,
    pub end_frame: usize,
    pub duration: Duration,
    pub position: Duration,
}

pub fn measure_gap_latency(audio: &RawAudio, gap_threshold_ms: f32) -> Vec<GapInfo> {
    let all_samples = audio.samples_f32();
    let num_channels = audio.channels as usize;
    let threshold = 100.0 / 32768.0;
    let mut gaps = Vec::new();
    let mut current_gap_start = None;

    for (i, chunk) in all_samples.chunks_exact(num_channels).enumerate() {
        let is_silent = chunk.iter().all(|&s| s.abs() < threshold);
        if is_silent {
            if current_gap_start.is_none() {
                current_gap_start = Some(i);
            }
        } else {
            if let Some(start) = current_gap_start {
                let duration_frames = i - start;
                let duration_ms = (duration_frames as f32 / audio.sample_rate as f32) * 1000.0;
                if duration_ms >= gap_threshold_ms {
                    gaps.push(GapInfo {
                        start_frame: start,
                        end_frame: i,
                        duration: Duration::from_secs_f64(duration_frames as f64 / audio.sample_rate as f64),
                        position: Duration::from_secs_f64(start as f64 / audio.sample_rate as f64),
                    });
                }
                current_gap_start = None;
            }
        }
    }

    // Check if there is an open gap at the end
    if let Some(start) = current_gap_start {
        let end = all_samples.len() / num_channels;
        let duration_frames = end - start;
        let duration_ms = (duration_frames as f32 / audio.sample_rate as f32) * 1000.0;
        if duration_ms >= gap_threshold_ms {
            gaps.push(GapInfo {
                start_frame: start,
                end_frame: end,
                duration: Duration::from_secs_f64(duration_frames as f64 / audio.sample_rate as f64),
                position: Duration::from_secs_f64(start as f64 / audio.sample_rate as f64),
            });
        }
    }

    gaps
}

#[allow(dead_code)]
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

pub fn align_audio(reference: &[f32], captured: &[f32], max_offset: usize) -> (usize, f64) {
    if reference.is_empty() || captured.is_empty() {
        return (0, 0.0);
    }

    let mut best_offset = 0;
    let mut max_corr = -1.0;

    let len = reference.len().min(captured.len() - max_offset);

    for offset in 0..=max_offset {
        let mut corr = 0.0;
        let mut ref_sum_sq = 0.0;
        let mut cap_sum_sq = 0.0;

        for i in 0..len {
            let r = reference[i];
            let c = captured[i + offset];
            corr += r * c;
            ref_sum_sq += r * r;
            cap_sum_sq += c * c;
        }

        let denom = (ref_sum_sq * cap_sum_sq).sqrt();
        let normalized_corr = if denom > 0.0 { corr / denom } else { 0.0 };

        if normalized_corr > max_corr {
            max_corr = normalized_corr;
            best_offset = offset;
        }
    }

    (best_offset, max_corr as f64)
}

pub fn compare_audio_exact(sent: &RawAudio, received: &RawAudio) -> CompareResult {
    let sent_samples = sent.samples_i16();
    let received_samples = received.samples_i16();

    let sent_frames = sent.num_frames();
    let received_frames = received.num_frames();

    // Alignment using f32 samples to find the offset
    let sent_f32 = sent.samples_f32();
    let received_f32 = received.samples_f32();
    let max_offset = (sent.sample_rate as usize * 2) * sent.channels as usize; // up to 2 seconds of offset

    // We only need to check alignment if sizes differ or basic match fails
    // Here we'll do a simplified alignment just to find leading silence in received
    let (offset, _) = align_audio(&sent_f32, &received_f32, max_offset.min(received_f32.len()));

    // Aligning samples
    let aligned_received = if offset < received_samples.len() {
        &received_samples[offset..]
    } else {
        &[]
    };

    let check_len = sent_samples.len().min(aligned_received.len());
    let mut matching_frames = 0;
    let mut first_mismatch_frame = None;
    let mut max_sample_diff = 0;
    let mut total_diff: f64 = 0.0;

    for i in 0..check_len {
        let diff = (sent_samples[i] as i32 - aligned_received[i] as i32).abs();
        if diff == 0 {
            matching_frames += 1;
        } else if first_mismatch_frame.is_none() {
            first_mismatch_frame = Some(i / sent.channels as usize);
        }

        if diff > max_sample_diff {
            max_sample_diff = diff;
        }
        total_diff += diff as f64;
    }

    let mean_sample_diff = if check_len > 0 {
        total_diff / check_len as f64
    } else {
        0.0
    };

    CompareResult {
        sample_count_match: (sent_frames as i64 - received_frames as i64).abs() <= 1,
        sent_frames,
        received_frames,
        matching_frames: matching_frames / sent.channels as usize,
        first_mismatch_frame,
        max_sample_diff,
        mean_sample_diff,
        bit_exact: max_sample_diff == 0 && check_len > 0,
    }
}

pub fn compute_snr(original: &RawAudio, received: &RawAudio) -> f64 {
    let orig_samples = original.samples_f32();
    let rec_samples = received.samples_f32();

    let max_offset = (original.sample_rate as usize * 2) * original.channels as usize;
    let (offset, _) = align_audio(&orig_samples, &rec_samples, max_offset.min(rec_samples.len()));

    let aligned_rec = if offset < rec_samples.len() {
        &rec_samples[offset..]
    } else {
        &[]
    };

    let len = orig_samples.len().min(aligned_rec.len());
    if len == 0 {
        return 0.0;
    }

    let mut signal_power = 0.0;
    let mut noise_power = 0.0;

    for i in 0..len {
        let s = orig_samples[i];
        let r = aligned_rec[i];
        let noise = s - r;

        signal_power += s * s;
        noise_power += noise * noise;
    }

    if noise_power == 0.0 {
        return f64::INFINITY; // Perfect match
    }

    10.0 * (signal_power as f64 / noise_power as f64).log10()
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub fn verify_codec_integrity(audio: &RawAudio, codec: CodecType, reference: Option<&RawAudio>) -> CodecVerifyResult {
    let mut snr_db = None;
    let mut bit_exact = None;
    let mut frame_count_correct = true;
    let mut issues = Vec::new();

    if let Some(ref_audio) = reference {
        let expected_frames = ref_audio.num_frames();
        let actual_frames = audio.num_frames();

        if (expected_frames as i64 - actual_frames as i64).abs() > 1 {
            frame_count_correct = false;
            issues.push(format!("Frame count mismatch: expected {}, got {}", expected_frames, actual_frames));
        }

        match codec {
            CodecType::Pcm | CodecType::Alac => {
                let comp = compare_audio_exact(ref_audio, audio);
                bit_exact = Some(comp.bit_exact);
                if !comp.bit_exact {
                    issues.push("Audio is not bit-exact".to_string());
                }
            }
            CodecType::Aac | CodecType::AacEld => {
                let snr = compute_snr(ref_audio, audio);
                snr_db = Some(snr);
                if snr < 40.0 {
                    issues.push(format!("SNR is too low: {:.2} dB (expected > 40 dB)", snr));
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

pub trait AudioCheck {
    fn report(&self) -> String;
}

impl AudioCheck for SineWaveResult {
    fn report(&self) -> String {
        format!(
            "Amplitude:\n  Min sample: {}\tMax sample: {}\n  RMS: {:.1}\tPeak: {:.1}\n  Crest factor: {:.3}\tDynamic range: {}\n\nFrequency (left channel):\n  Measured estimate: {:.2} Hz\n  Error: {:.2}%\n\nContinuity:\n  Max silence run: {} samples ({:.2} ms)",
            self.min_sample, self.max_sample, self.rms, self.peak, self.crest_factor, self.amplitude_range, self.measured_frequency, self.frequency_error_pct, self.max_silence_run_samples, self.max_silence_run_ms
        )
    }
}

pub fn audio_diagnostic_report(audio: &RawAudio, filename: &str, checks: &[Box<dyn AudioCheck>], codec_res: Option<&CodecVerifyResult>) -> String {
    let mut report = String::new();
    report.push_str("Audio Diagnostic Report\n");
    report.push_str("=======================\n");
    report.push_str(&format!("File: {}\n", filename));

    let endian_str = match audio.endianness {
        Endianness::Little => "LE",
        Endianness::Big => "BE",
    };
    let channels_str = match audio.channels {
        1 => "mono",
        2 => "stereo",
        _ => "multichannel",
    };
    report.push_str(&format!("Format: {}-bit {} {} @ {} Hz\n", audio.bits_per_sample, endian_str, channels_str, audio.sample_rate));
    report.push_str(&format!("Duration: {:.2}s ({} frames)\n", audio.duration().as_secs_f64(), audio.num_frames()));
    report.push_str(&format!("Data size: {} bytes\n\n", audio.data.len()));

    for check in checks {
        report.push_str(&check.report());
        report.push_str("\n\n");
    }

    if let Some(c_res) = codec_res {
        report.push_str(&format!("Codec: {:?}\n", c_res.codec));
        if let Some(be) = c_res.bit_exact {
            report.push_str(&format!("  Bit-exact match: {}\n", if be { "yes" } else { "no" }));
        }
        if let Some(snr) = c_res.snr_db {
            report.push_str(&format!("  SNR: {:.2} dB\n", snr));
        }
        report.push_str(&format!("  Frame count correct: {}\n", if c_res.frame_count_correct { "yes" } else { "no" }));
        for issue in &c_res.issues {
            report.push_str(&format!("  Issue: {}\n", issue));
        }
        report.push_str("\n");
    }

    report
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawAudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub endianness: Endianness,
    pub signed: bool,
}

#[allow(dead_code)]
impl RawAudioFormat {
    #[allow(dead_code)]
    pub const CD_QUALITY: Self = Self {
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };
    #[allow(dead_code)]
    pub const HIRES: Self = Self {
        sample_rate: 48000,
        channels: 2,
        bits_per_sample: 24,
        endianness: Endianness::Little,
        signed: true,
    };
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RawAudio {
    pub data: Vec<u8>,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub endianness: Endianness,
    pub signed: bool,
}

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum AudioVerifyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid audio format: {0}")]
    InvalidFormat(String),
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
}

impl RawAudio {
    #[allow(dead_code)]
    pub fn from_file(path: &Path, format: RawAudioFormat) -> Result<Self, AudioVerifyError> {
        let data = std::fs::read(path)?;
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

    pub fn num_frames(&self) -> usize {
        let bytes_per_frame = (self.channels as usize * self.bits_per_sample as usize) / 8;
        if bytes_per_frame == 0 {
            0
        } else {
            self.data.len() / bytes_per_frame
        }
    }

    pub fn duration(&self) -> Duration {
        if self.sample_rate == 0 {
            return Duration::from_secs(0);
        }
        let frames = self.num_frames();
        let secs = frames as f64 / self.sample_rate as f64;
        Duration::from_secs_f64(secs)
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn samples_i16(&self) -> Vec<i16> {
        let mut samples = Vec::new();
        let bytes_per_sample = (self.bits_per_sample / 8) as usize;

        for chunk in self.data.chunks_exact(bytes_per_sample) {
            let sample = if self.bits_per_sample == 16 {
                if self.endianness == Endianness::Little {
                    i16::from_le_bytes([chunk[0], chunk[1]])
                } else {
                    i16::from_be_bytes([chunk[0], chunk[1]])
                }
            } else if self.bits_per_sample == 24 {
                if self.endianness == Endianness::Little {
                    let val = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], if chunk[2] & 0x80 != 0 { 0xFF } else { 0 }]);
                    (val >> 8) as i16
                } else {
                    let val = i32::from_be_bytes([if chunk[0] & 0x80 != 0 { 0xFF } else { 0 }, chunk[0], chunk[1], chunk[2]]);
                    (val >> 8) as i16
                }
            } else {
                0
            };
            samples.push(sample);
        }
        samples
    }

    pub fn samples_f32(&self) -> Vec<f32> {
        let mut samples = Vec::new();
        let bytes_per_sample = (self.bits_per_sample / 8) as usize;

        for chunk in self.data.chunks_exact(bytes_per_sample) {
            let sample = if self.bits_per_sample == 16 {
                let val = if self.endianness == Endianness::Little {
                    i16::from_le_bytes([chunk[0], chunk[1]])
                } else {
                    i16::from_be_bytes([chunk[0], chunk[1]])
                };
                val as f32 / 32768.0
            } else if self.bits_per_sample == 24 {
                let val = if self.endianness == Endianness::Little {
                    i32::from_le_bytes([chunk[0], chunk[1], chunk[2], if chunk[2] & 0x80 != 0 { 0xFF } else { 0 }])
                } else {
                    i32::from_be_bytes([if chunk[0] & 0x80 != 0 { 0xFF } else { 0 }, chunk[0], chunk[1], chunk[2]])
                };
                val as f32 / 8388608.0
            } else {
                0.0
            };
            samples.push(sample);
        }
        samples
    }

    pub fn channel(&self, ch: usize) -> Vec<f32> {
        let all_samples = self.samples_f32();
        let num_channels = self.channels as usize;
        if ch >= num_channels {
            return Vec::new();
        }
        all_samples.into_iter().skip(ch).step_by(num_channels).collect()
    }
}
