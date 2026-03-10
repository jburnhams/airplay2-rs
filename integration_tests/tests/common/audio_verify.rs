#![allow(dead_code)]
use std::fs;
use std::path::Path;
use std::time::Duration;

// Mock framework structures based on spec
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
    pub const CD_QUALITY: RawAudioFormat = RawAudioFormat {
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        endianness: Endianness::Little,
        signed: true,
    };

    pub const HIRES: RawAudioFormat = RawAudioFormat {
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
pub struct AudioError(pub String);

impl RawAudio {
    pub fn from_file(path: &Path, format: RawAudioFormat) -> Result<Self, AudioError> {
        let data = fs::read(path).map_err(|e| AudioError(e.to_string()))?;
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

    pub fn samples_i16(&self) -> Vec<i16> {
        // Assume LE for tests
        let mut out = Vec::with_capacity(self.data.len() / 2);
        for chunk in self.data.chunks_exact(2) {
            let val = i16::from_le_bytes([chunk[0], chunk[1]]);
            out.push(val);
        }
        out
    }

    pub fn samples_f32(&self) -> Vec<f32> {
        self.samples_i16()
            .into_iter()
            .map(|s| s as f32 / 32768.0)
            .collect()
    }

    pub fn channel(&self, ch: usize) -> Vec<f32> {
        let f32_samples = self.samples_f32();
        f32_samples
            .into_iter()
            .skip(ch)
            .step_by(self.channels as usize)
            .collect()
    }

    pub fn duration(&self) -> Duration {
        let frames = self.num_frames();
        Duration::from_secs_f64(frames as f64 / self.sample_rate as f64)
    }

    pub fn num_frames(&self) -> usize {
        let bytes_per_sample = (self.bits_per_sample / 8) as usize;
        let bytes_per_frame = bytes_per_sample * self.channels as usize;
        self.data.len().checked_div(bytes_per_frame).unwrap_or(0)
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
pub struct AudioVerifyError(pub String);

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

impl SineWaveCheck {
    pub fn verify(&self, audio: &RawAudio) -> Result<SineWaveResult, AudioVerifyError> {
        let samples = audio.samples_i16();
        let channel = self.channel.unwrap_or(0);
        let ch_samples: Vec<i16> = samples
            .into_iter()
            .skip(channel)
            .step_by(audio.channels as usize)
            .collect();

        if ch_samples.is_empty() {
            return Err(AudioVerifyError("No samples found".to_string()));
        }

        let mut min_sample = i16::MAX;
        let mut max_sample = i16::MIN;
        let mut sum_sq = 0.0;
        let mut max_silence_run = 0;
        let mut current_silence_run = 0;

        let mut zero_crossings = 0;
        let mut prev_sign = ch_samples[0] >= 0;

        for &sample in &ch_samples {
            if sample < min_sample {
                min_sample = sample;
            }
            if sample > max_sample {
                max_sample = sample;
            }

            let s_f32 = sample as f32;
            sum_sq += s_f32 * s_f32;

            if sample.abs() < 100 {
                current_silence_run += 1;
                if current_silence_run > max_silence_run {
                    max_silence_run = current_silence_run;
                }
            } else {
                current_silence_run = 0;
            }

            let sign = sample >= 0;
            if sign != prev_sign {
                zero_crossings += 1;
                prev_sign = sign;
            }
        }

        let num_frames = ch_samples.len();
        let duration = Duration::from_secs_f64(num_frames as f64 / audio.sample_rate as f64);

        // Approximate active duration by subtracting the longest silence run
        let active_duration_secs =
            (num_frames.saturating_sub(max_silence_run)) as f32 / audio.sample_rate as f32;

        let measured_frequency = if active_duration_secs > 0.0 {
            zero_crossings as f32 / (2.0 * active_duration_secs)
        } else {
            0.0
        };

        let rms = (sum_sq / num_frames as f32).sqrt();
        let peak = (max_sample as f32).max((min_sample as f32).abs());
        let crest_factor = if rms > 0.0 { peak / rms } else { 0.0 };

        let max_silence_run_ms = (max_silence_run as f32 / audio.sample_rate as f32) * 1000.0;

        let mut failure_reasons = Vec::new();

        let freq_error_pct = if self.expected_frequency > 0.0 {
            ((measured_frequency - self.expected_frequency).abs() / self.expected_frequency) * 100.0
        } else {
            0.0
        };

        if self.check_frequency && freq_error_pct > self.frequency_tolerance_pct {
            failure_reasons.push(format!(
                "Frequency error {:.2}% > {:.2}% (expected {}, got {})",
                freq_error_pct,
                self.frequency_tolerance_pct,
                self.expected_frequency,
                measured_frequency
            ));
        }

        if self.check_amplitude && peak < self.min_amplitude as f32 {
            failure_reasons.push(format!("Amplitude {} < min {}", peak, self.min_amplitude));
        }

        if self.check_continuity && max_silence_run_ms > self.max_silence_run_ms {
            failure_reasons.push(format!(
                "Silence run {} ms > max {} ms",
                max_silence_run_ms, self.max_silence_run_ms
            ));
        }

        Ok(SineWaveResult {
            measured_frequency,
            frequency_error_pct: freq_error_pct,
            min_sample,
            max_sample,
            amplitude_range: max_sample as i32 - min_sample as i32,
            rms,
            peak,
            crest_factor,
            max_silence_run_samples: max_silence_run,
            max_silence_run_ms,
            num_frames,
            duration,
            passed: failure_reasons.is_empty(),
            failure_reasons,
        })
    }
}

impl SineWaveResult {
    pub fn assert_passed(&self) -> Result<(), AudioVerifyError> {
        if self.passed {
            Ok(())
        } else {
            Err(AudioVerifyError(format!(
                "Checks failed: {:?}",
                self.failure_reasons
            )))
        }
    }
}

pub fn audio_diagnostic_report(audio: &RawAudio, res: &SineWaveResult) -> String {
    format!(
        "Audio Diagnostic Report\n=======================\nFormat: {}-bit {:?} {:?} ch @ {} \
         Hz\nDuration: {:.2}s ({} frames)\nData size: {} bytes\n\nAmplitude:\n  Min: {} Max: {}\n  \
         RMS: {:.2} Peak: {:.2}\n  Crest: {:.2} Range: {}\n\nFrequency: {:.2} Hz (err \
         {:.2}%)\n\nPassed: {}\nReasons: {:?}",
        audio.bits_per_sample,
        audio.endianness,
        audio.channels,
        audio.sample_rate,
        res.duration.as_secs_f32(),
        audio.num_frames(),
        audio.data.len(),
        res.min_sample,
        res.max_sample,
        res.rms,
        res.peak,
        res.crest_factor,
        res.amplitude_range,
        res.measured_frequency,
        res.frequency_error_pct,
        res.passed,
        res.failure_reasons
    )
}
