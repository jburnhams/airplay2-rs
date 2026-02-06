# Section 65: Audio Verification & Analysis Framework

## Dependencies
- **Section 64**: Subprocess Management Framework
- `tests/common/python_receiver.rs` — existing `verify_sine_wave_quality` and `AudioAnalysis`

## Overview

Every integration test ultimately asserts that audio arrived correctly. This section defines a shared audio verification framework that works across all test suites — regardless of whether audio is captured by the Python receiver, shairport-sync's pipe backend, our own receiver's file sink, or pyatv's recording mode. The framework must handle different raw formats, perform codec-specific checks, and produce clear diagnostic output on failure.

## Objectives

- Normalise raw audio from various tools into a common analysis format
- Frequency-domain and time-domain verification of sine wave test signals
- Codec-specific integrity checks (ALAC frame boundaries, AAC packet completeness)
- Latency and timing measurements between send and receive
- Detailed diagnostic output with waveform statistics on failure
- Bit-exact comparison mode for lossless codecs

---

## Tasks

### 65.1 Audio Format Normalisation

**File:** `tests/common/audio_verify.rs`

Different tools output audio in different raw formats:

| Tool | Output Format | Notes |
|---|---|---|
| Python receiver | `received_audio_44100_2ch.raw` — 16-bit LE stereo interleaved | Existing, read in `python_receiver.rs:209` |
| shairport-sync | Pipe backend: raw 16-bit signed LE interleaved, or `stdout` backend | Format depends on config; `--output=pipe` writes to named pipe |
| Our receiver (file sink) | Configurable — default 16-bit LE stereo interleaved | Must match `AudioFormat` config |
| pyatv | N/A — pyatv is the sender, our receiver captures | We verify our own receiver output |

**Struct: `RawAudio`**

Fields:
- `data: Vec<u8>` — raw byte data
- `sample_rate: u32` — e.g., 44100, 48000
- `channels: u16` — 1 (mono), 2 (stereo)
- `bits_per_sample: u16` — 16 or 24
- `endianness: Endianness` — `Little` or `Big`
- `signed: bool` — always true for our use cases

Methods:
- `fn from_file(path: &Path, format: RawAudioFormat) -> Result<Self, AudioError>` — read file with known format.
- `fn from_bytes(data: Vec<u8>, format: RawAudioFormat) -> Self` — wrap existing data.
- `fn samples_i16(&self) -> Vec<i16>` — convert to i16 samples (left channel only for analysis, or interleaved).
- `fn samples_f32(&self) -> Vec<f32>` — convert to normalised float samples (-1.0 to 1.0).
- `fn channel(&self, ch: usize) -> Vec<f32>` — extract single channel as f32.
- `fn duration(&self) -> Duration` — compute from data length, sample rate, channels, bit depth.
- `fn num_frames(&self) -> usize` — number of audio frames (samples / channels).
- `fn is_empty(&self) -> bool`

**Struct: `RawAudioFormat`**

Fields: `sample_rate`, `channels`, `bits_per_sample`, `endianness`, `signed`.

Predefined constants:
- `RawAudioFormat::CD_QUALITY` — 44100 Hz, 2 ch, 16-bit, little-endian, signed
- `RawAudioFormat::HIRES` — 48000 Hz, 2 ch, 24-bit, little-endian, signed

---

### 65.2 Sine Wave Verification

**File:** `tests/common/audio_verify.rs`

This generalises `ReceiverOutput::verify_sine_wave_quality` from `python_receiver.rs:287–409`.

**Struct: `SineWaveCheck`**

Fields:
- `expected_frequency: f32` — Hz
- `frequency_tolerance_pct: f32` — default 5.0 (5%)
- `min_amplitude: i16` — default 20000 (for full-scale test signals)
- `max_silence_run_ms: f32` — default 100.0 ms
- `check_frequency: bool` — default true
- `check_continuity: bool` — default true
- `check_amplitude: bool` — default true
- `channel: Option<usize>` — specific channel to check, or None for left

Methods:
- `fn verify(&self, audio: &RawAudio) -> Result<SineWaveResult, AudioVerifyError>`

**Struct: `SineWaveResult`**

Fields:
- `measured_frequency: f32` — estimated from zero crossings
- `frequency_error_pct: f32`
- `min_sample: i16`
- `max_sample: i16`
- `amplitude_range: i32`
- `rms: f32`
- `peak: f32`
- `crest_factor: f32`
- `max_silence_run_samples: usize`
- `max_silence_run_ms: f32`
- `num_frames: usize`
- `duration: Duration`
- `passed: bool`
- `failure_reasons: Vec<String>`

Method:
- `fn assert_passed(&self) -> Result<(), AudioVerifyError>` — returns `Err` with detailed diagnostics if any check failed.

**Frequency estimation algorithm:**

The existing zero-crossing approach (`python_receiver.rs:316–323`) works for clean sine waves but is inaccurate for noisy or clipped signals. Enhance with:

1. **Zero-crossing (primary)** — count sign changes, divide by (2 * duration). Fast, no dependencies.
2. **Autocorrelation (fallback)** — compute autocorrelation of the signal, find first peak after zero lag. More robust to noise. O(N*max_lag) but max_lag can be bounded to `sample_rate / min_frequency`.
3. If both methods agree within tolerance, report zero-crossing result. If they disagree, report both and flag a warning.

**Edge cases:**
- DC offset in captured audio — subtract mean before analysis.
- Initial silence (setup latency) — skip first N ms of audio (configurable, default 200ms).
- Trailing silence (teardown) — truncate trailing zeros before analysis.
- Codec-introduced artifacts at start/end — ALAC and AAC encoders may produce a few silent frames at the start.
- Very short audio (<500ms) — lower confidence in frequency estimation; still check amplitude.

---

### 65.3 Multi-Frequency Verification

**File:** `tests/common/audio_verify.rs`

Some tests send different frequencies to left and right channels (as done in existing tests for stereo verification).

**Struct: `StereoSineCheck`**

Fields:
- `left_frequency: f32`
- `right_frequency: f32`
- `frequency_tolerance_pct: f32`

Method:
- `fn verify(&self, audio: &RawAudio) -> Result<StereoSineResult, AudioVerifyError>` — runs `SineWaveCheck` independently on each channel and verifies the frequencies are distinct and correct.

**Struct: `StereoSineResult`**

Fields:
- `left: SineWaveResult`
- `right: SineWaveResult`

---

### 65.4 Bit-Exact Comparison (Lossless Codecs)

**File:** `tests/common/audio_verify.rs`

For PCM and ALAC (lossless), the received audio should be bit-exact (or very close) to the sent audio.

**Function: `fn compare_audio_exact(sent: &RawAudio, received: &RawAudio) -> CompareResult`**

**Struct: `CompareResult`**

Fields:
- `sample_count_match: bool`
- `sent_frames: usize`
- `received_frames: usize`
- `matching_frames: usize`
- `first_mismatch_frame: Option<usize>`
- `max_sample_diff: i32`
- `mean_sample_diff: f64`
- `bit_exact: bool`

**Implementation notes:**
- Account for leading/trailing silence introduced by codec framing. Align signals by finding the cross-correlation peak.
- For PCM, expect bit-exact after alignment.
- For ALAC, expect bit-exact after alignment (lossless).
- For AAC (lossy), bit-exact is not expected; use SNR measurement instead.

**Function: `fn compute_snr(original: &RawAudio, received: &RawAudio) -> f64`**

Returns signal-to-noise ratio in dB. For lossy codecs, expect SNR > 40 dB for a clean sine wave.

**Function: `fn align_audio(reference: &[f32], captured: &[f32], max_offset: usize) -> (usize, f64)`**

Returns `(offset, correlation)` — the sample offset that maximises cross-correlation between the two signals. `max_offset` bounds the search window (e.g., `sample_rate * 2` for up to 2 seconds of leading silence).

---

### 65.5 Timing & Latency Measurement

**File:** `tests/common/audio_verify.rs`

**Function: `fn measure_onset_latency(audio: &RawAudio, threshold: f32) -> Duration`**

Find the first frame where amplitude exceeds `threshold` (as fraction of full-scale). Returns the time from start of capture to first audible sample.

**Function: `fn measure_gap_latency(audio: &RawAudio, gap_threshold_ms: f32) -> Vec<GapInfo>`**

Find silent gaps in the audio stream that exceed `gap_threshold_ms`.

**Struct: `GapInfo`**

Fields:
- `start_frame: usize`
- `end_frame: usize`
- `duration: Duration`
- `position: Duration` — offset from audio start

Use cases:
- Verify onset latency is within expected bounds (e.g., <3 seconds for AP1, <2 seconds for AP2).
- Verify no unexpected gaps during sustained streaming.
- Detect audio dropout during volume changes or metadata updates.

---

### 65.6 Codec-Specific Verification

**File:** `tests/common/audio_verify.rs`

**PCM verification:**
- Bit-exact comparison with source signal.
- Verify sample count matches expected duration within ±1 frame.
- Verify no byte-swapping issues (endianness).

**ALAC verification:**
- Bit-exact comparison (lossless codec).
- Verify decoder produced correct number of frames.
- Check that ALAC magic cookie / frame header is consistent (if captured at RTP level).

**AAC verification:**
- SNR check (>40 dB for clean sine wave).
- Frequency within tolerance (AAC may shift slightly).
- No audible clicks or pops (max sample-to-sample delta check).

**Function: `fn verify_codec_integrity(audio: &RawAudio, codec: CodecType, reference: Option<&RawAudio>) -> CodecVerifyResult`**

**Enum: `CodecType`** — `Pcm`, `Alac`, `Aac`, `AacEld`

**Struct: `CodecVerifyResult`**

Fields:
- `codec: CodecType`
- `snr_db: Option<f64>` — only for lossy codecs
- `bit_exact: Option<bool>` — only for lossless codecs
- `frame_count_correct: bool`
- `issues: Vec<String>`

---

### 65.7 Diagnostic Reporting

**File:** `tests/common/audio_verify.rs`

When a verification fails, produce a detailed report.

**Function: `fn audio_diagnostic_report(audio: &RawAudio, checks: &[Box<dyn AudioCheck>]) -> String`**

Produces a multi-line report:
```
Audio Diagnostic Report
=======================
File: received_audio_44100_2ch.raw
Format: 16-bit LE stereo @ 44100 Hz
Duration: 3.21s (141,561 frames)
Data size: 566,244 bytes

Amplitude:
  Min sample: -32765    Max sample: 32766
  RMS: 23170.4          Peak: 32766.0
  Crest factor: 1.414   Dynamic range: 65531

Frequency (left channel):
  Zero-crossing estimate: 440.2 Hz
  Autocorrelation estimate: 439.8 Hz
  Expected: 440.0 Hz    Error: 0.05%

Continuity:
  Max silence run: 0 samples (0.00 ms)
  Gaps > 10ms: 0

Timing:
  Onset latency: 12.5 ms
  Trailing silence: 45.2 ms

Codec: PCM
  Bit-exact match: yes (after 552-sample offset alignment)

RESULT: PASS
```

---

## Test Cases

| ID | Test | Verifies |
|---|---|---|
| 65-T1 | `test_sine_wave_verify_clean` | 440 Hz sine at full amplitude passes all checks |
| 65-T2 | `test_sine_wave_wrong_frequency` | 440 Hz expected, 880 Hz received → frequency check fails |
| 65-T3 | `test_sine_wave_low_amplitude` | Half-amplitude signal → amplitude check fails |
| 65-T4 | `test_sine_wave_with_silence_gap` | 1-second gap mid-stream → continuity check fails |
| 65-T5 | `test_sine_wave_leading_silence` | 500ms of silence then signal → onset latency measured correctly |
| 65-T6 | `test_stereo_independent_channels` | 440 Hz left, 880 Hz right → both detected independently |
| 65-T7 | `test_bit_exact_pcm` | PCM round-trip → CompareResult.bit_exact == true |
| 65-T8 | `test_bit_exact_alac` | ALAC encode/decode → CompareResult.bit_exact == true after alignment |
| 65-T9 | `test_lossy_aac_snr` | AAC encode/decode → SNR > 40 dB |
| 65-T10 | `test_align_audio_with_offset` | 200-sample offset → alignment finds correct offset |
| 65-T11 | `test_raw_audio_format_cd` | Load raw file as CD quality, verify sample count |
| 65-T12 | `test_raw_audio_24bit` | Load 24-bit raw audio, verify conversion to f32 |
| 65-T13 | `test_dc_offset_removal` | Sine wave with DC offset → frequency still estimated correctly |
| 65-T14 | `test_very_short_audio` | 100ms clip → graceful handling with warnings |
| 65-T15 | `test_diagnostic_report_format` | Verify report string contains expected sections |
| 65-T16 | `test_gap_detection` | Audio with two 50ms gaps → both reported |

---

## Migration Path

The existing `ReceiverOutput::verify_sine_wave_quality` (python_receiver.rs:287–409) and `ReceiverOutput::analyze_audio_detailed` (414–463) should be migrated to use this framework:

1. `verify_sine_wave_quality` → construct `SineWaveCheck` and call `verify()` on `RawAudio::from_file(...)`.
2. `analyze_audio_detailed` → replaced by `SineWaveResult` which contains all the same fields plus more.
3. `AudioAnalysis` struct → replaced by `SineWaveResult`.
4. `TestSineSource` → unchanged (it generates the test signal, not verifies it).

Existing tests should continue to work through thin wrapper methods on `ReceiverOutput` that delegate to the new framework.

---

## Acceptance Criteria

- [ ] `RawAudio` can load files from Python receiver, shairport-sync pipe output, and our receiver's file sink
- [ ] Sine wave verification works for 440 Hz, 880 Hz, 1000 Hz at 44100 and 48000 Hz sample rates
- [ ] Bit-exact comparison passes for PCM and ALAC round-trips
- [ ] SNR measurement gives reasonable values for AAC
- [ ] Diagnostic report is clear enough to debug failures without re-running the test
- [ ] All existing integration tests continue to pass after migration

---

## References

- `tests/common/python_receiver.rs` lines 287–463 — existing verification logic
- `src/audio/format.rs` — `AudioFormat`, `SampleRate` types
- `src/streaming/source.rs` — `AudioSource` trait
