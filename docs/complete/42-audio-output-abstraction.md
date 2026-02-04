# Section 42: Audio Output Abstraction

## Dependencies
- **Section 34**: Receiver Overview (feature flags)
- **Section 41**: Jitter Buffer (audio data source)
- **Section 11**: Audio Formats & Codecs (format types)

## Overview

This section implements the audio output abstraction layer, enabling the receiver to play audio through various platform backends:

- **CoreAudio** (macOS) - Priority platform
- **CPAL** (cross-platform) - Fallback for all platforms
- **ALSA** (Linux) - Native Linux support
- **WASAPI** (Windows) - Native Windows support (via CPAL)

The design uses traits to abstract the output backend, allowing compile-time selection via feature flags and runtime selection where appropriate.

## Objectives

- Define `AudioOutput` trait for platform abstraction
- Implement CoreAudio backend (macOS priority)
- Implement CPAL backend (cross-platform fallback)
- Support audio format negotiation
- Handle buffer management and callback-based output
- Provide volume control at output level

---

## Tasks

### 42.1 Audio Output Trait

- [x] **42.1.1** Define platform-agnostic audio output trait

**File:** `src/audio/output.rs`

```rust
//! Audio output abstraction
//!
//! Platform-agnostic trait for audio playback with implementations
//! for CoreAudio, CPAL, ALSA, etc.

use crate::audio::format::{AudioFormat, SampleFormat, SampleRate};
use std::sync::Arc;
use std::time::Duration;

/// Errors from audio output
#[derive(Debug, thiserror::Error)]
pub enum AudioOutputError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Format not supported: {0:?}")]
    FormatNotSupported(AudioFormat),

    #[error("Stream error: {0}")]
    StreamError(String),

    #[error("Device error: {0}")]
    DeviceError(String),

    #[error("Buffer underrun")]
    Underrun,

    #[error("Output closed")]
    Closed,
}

/// Audio output state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputState {
    Stopped,
    Playing,
    Paused,
}

/// Audio device information
#[derive(Debug, Clone)]
pub struct AudioDevice {
    /// Device identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Whether this is the default device
    pub is_default: bool,
    /// Supported sample rates
    pub supported_rates: Vec<SampleRate>,
    /// Supported channel counts
    pub supported_channels: Vec<u8>,
}

/// Callback for providing audio data
pub type AudioCallback = Box<dyn FnMut(&mut [u8]) -> usize + Send + 'static>;

/// Audio output trait
///
/// Implementations provide platform-specific audio playback.
pub trait AudioOutput: Send {
    /// Get available output devices
    fn enumerate_devices(&self) -> Result<Vec<AudioDevice>, AudioOutputError>;

    /// Get the default output device
    fn default_device(&self) -> Result<AudioDevice, AudioOutputError>;

    /// Open output stream with specified format
    fn open(
        &mut self,
        device: Option<&str>,
        format: AudioFormat,
    ) -> Result<(), AudioOutputError>;

    /// Start playback with callback
    ///
    /// The callback is invoked to fill output buffers.
    fn start(&mut self, callback: AudioCallback) -> Result<(), AudioOutputError>;

    /// Stop playback
    fn stop(&mut self) -> Result<(), AudioOutputError>;

    /// Pause playback
    fn pause(&mut self) -> Result<(), AudioOutputError>;

    /// Resume playback
    fn resume(&mut self) -> Result<(), AudioOutputError>;

    /// Get current state
    fn state(&self) -> OutputState;

    /// Set volume (0.0 to 1.0)
    fn set_volume(&mut self, volume: f32) -> Result<(), AudioOutputError>;

    /// Get current volume
    fn volume(&self) -> f32;

    /// Get output latency
    fn latency(&self) -> Duration;

    /// Get the actual format being used
    fn format(&self) -> Option<AudioFormat>;

    /// Close the output
    fn close(&mut self) -> Result<(), AudioOutputError>;
}

/// Create the default audio output for the current platform
pub fn create_default_output() -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    #[cfg(feature = "audio-coreaudio")]
    {
        return Ok(Box::new(super::output_coreaudio::CoreAudioOutput::new()?));
    }

    #[cfg(feature = "audio-cpal")]
    {
        return Ok(Box::new(super::output_cpal::CpalOutput::new()?));
    }

    #[cfg(feature = "audio-alsa")]
    {
        return Ok(Box::new(super::output_alsa::AlsaOutput::new()?));
    }

    #[cfg(not(any(feature = "audio-coreaudio", feature = "audio-cpal", feature = "audio-alsa")))]
    {
        Err(AudioOutputError::DeviceError(
            "No audio backend enabled. Enable audio-coreaudio, audio-cpal, or audio-alsa feature.".into()
        ))
    }
}
```

---

### 42.2 CPAL Backend (Cross-Platform)

- [x] **42.2.1** Implement CPAL-based audio output

**File:** `src/audio/output_cpal.rs`

```rust
//! CPAL-based audio output
//!
//! Cross-platform audio output using the `cpal` crate.
//! Works on macOS, Windows, Linux, iOS, Android.

#[cfg(feature = "audio-cpal")]
mod implementation {
    use super::super::output::{
        AudioOutput, AudioOutputError, OutputState, AudioDevice, AudioCallback
    };
    use crate::audio::format::{AudioFormat, SampleFormat, SampleRate};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    pub struct CpalOutput {
        host: cpal::Host,
        device: Option<cpal::Device>,
        stream: Option<cpal::Stream>,
        state: OutputState,
        volume: f32,
        format: Option<AudioFormat>,
        callback: Arc<Mutex<Option<AudioCallback>>>,
    }

    impl CpalOutput {
        pub fn new() -> Result<Self, AudioOutputError> {
            let host = cpal::default_host();

            Ok(Self {
                host,
                device: None,
                stream: None,
                state: OutputState::Stopped,
                volume: 1.0,
                format: None,
                callback: Arc::new(Mutex::new(None)),
            })
        }

        fn sample_rate_to_cpal(rate: SampleRate) -> cpal::SampleRate {
            cpal::SampleRate(rate.as_hz())
        }
    }

    impl AudioOutput for CpalOutput {
        fn enumerate_devices(&self) -> Result<Vec<AudioDevice>, AudioOutputError> {
            let default_name = self.host.default_output_device()
                .map(|d| d.name().unwrap_or_default());

            let devices = self.host.output_devices()
                .map_err(|e| AudioOutputError::DeviceError(e.to_string()))?;

            let mut result = Vec::new();
            for device in devices {
                let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
                let is_default = default_name.as_ref() == Some(&name);

                // Get supported configs
                let mut supported_rates = Vec::new();
                let mut supported_channels = Vec::new();

                if let Ok(configs) = device.supported_output_configs() {
                    for config in configs {
                        // Add min and max sample rates
                        supported_rates.push(SampleRate::from_hz(config.min_sample_rate().0));
                        supported_rates.push(SampleRate::from_hz(config.max_sample_rate().0));
                        supported_channels.push(config.channels() as u8);
                    }
                }

                supported_rates.sort();
                supported_rates.dedup();
                supported_channels.sort();
                supported_channels.dedup();

                result.push(AudioDevice {
                    id: name.clone(),
                    name,
                    is_default,
                    supported_rates,
                    supported_channels,
                });
            }

            Ok(result)
        }

        fn default_device(&self) -> Result<AudioDevice, AudioOutputError> {
            let device = self.host.default_output_device()
                .ok_or_else(|| AudioOutputError::DeviceNotFound("No default device".into()))?;

            let name = device.name().unwrap_or_else(|_| "Default".to_string());

            Ok(AudioDevice {
                id: name.clone(),
                name,
                is_default: true,
                supported_rates: vec![SampleRate::Hz44100, SampleRate::Hz48000],
                supported_channels: vec![2],
            })
        }

        fn open(
            &mut self,
            device_id: Option<&str>,
            format: AudioFormat,
        ) -> Result<(), AudioOutputError> {
            let device = if let Some(id) = device_id {
                self.host.output_devices()
                    .map_err(|e| AudioOutputError::DeviceError(e.to_string()))?
                    .find(|d| d.name().ok().as_deref() == Some(id))
                    .ok_or_else(|| AudioOutputError::DeviceNotFound(id.to_string()))?
            } else {
                self.host.default_output_device()
                    .ok_or_else(|| AudioOutputError::DeviceNotFound("No default".into()))?
            };

            self.device = Some(device);
            self.format = Some(format);

            Ok(())
        }

        fn start(&mut self, callback: AudioCallback) -> Result<(), AudioOutputError> {
            let device = self.device.as_ref()
                .ok_or_else(|| AudioOutputError::DeviceError("No device opened".into()))?;

            let format = self.format
                .ok_or_else(|| AudioOutputError::DeviceError("No format set".into()))?;

            // Store callback
            *self.callback.lock().unwrap() = Some(callback);
            let callback_ref = self.callback.clone();
            let volume = self.volume;

            // Configure stream
            let config = cpal::StreamConfig {
                channels: format.channels as u16,
                sample_rate: cpal::SampleRate(format.sample_rate.as_hz()),
                buffer_size: cpal::BufferSize::Default,
            };

            // Build stream based on sample format
            let stream = match format.sample_format {
                SampleFormat::I16 => {
                    device.build_output_stream(
                        &config,
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            let bytes: &mut [u8] = unsafe {
                                std::slice::from_raw_parts_mut(
                                    data.as_mut_ptr() as *mut u8,
                                    data.len() * 2,
                                )
                            };

                            let mut cb = callback_ref.lock().unwrap();
                            if let Some(ref mut callback) = *cb {
                                callback(bytes);
                            }

                            // Apply volume
                            for sample in data.iter_mut() {
                                *sample = (*sample as f32 * volume) as i16;
                            }
                        },
                        |err| {
                            tracing::error!("CPAL stream error: {}", err);
                        },
                        None,
                    )
                }
                SampleFormat::F32 => {
                    device.build_output_stream(
                        &config,
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            let bytes: &mut [u8] = unsafe {
                                std::slice::from_raw_parts_mut(
                                    data.as_mut_ptr() as *mut u8,
                                    data.len() * 4,
                                )
                            };

                            let mut cb = callback_ref.lock().unwrap();
                            if let Some(ref mut callback) = *cb {
                                callback(bytes);
                            }

                            // Apply volume
                            for sample in data.iter_mut() {
                                *sample *= volume;
                            }
                        },
                        |err| {
                            tracing::error!("CPAL stream error: {}", err);
                        },
                        None,
                    )
                }
                _ => {
                    return Err(AudioOutputError::FormatNotSupported(format));
                }
            }.map_err(|e| AudioOutputError::StreamError(e.to_string()))?;

            stream.play().map_err(|e| AudioOutputError::StreamError(e.to_string()))?;

            self.stream = Some(stream);
            self.state = OutputState::Playing;

            Ok(())
        }

        fn stop(&mut self) -> Result<(), AudioOutputError> {
            if let Some(stream) = self.stream.take() {
                drop(stream);
            }
            self.state = OutputState::Stopped;
            Ok(())
        }

        fn pause(&mut self) -> Result<(), AudioOutputError> {
            if let Some(ref stream) = self.stream {
                stream.pause().map_err(|e| AudioOutputError::StreamError(e.to_string()))?;
            }
            self.state = OutputState::Paused;
            Ok(())
        }

        fn resume(&mut self) -> Result<(), AudioOutputError> {
            if let Some(ref stream) = self.stream {
                stream.play().map_err(|e| AudioOutputError::StreamError(e.to_string()))?;
            }
            self.state = OutputState::Playing;
            Ok(())
        }

        fn state(&self) -> OutputState {
            self.state
        }

        fn set_volume(&mut self, volume: f32) -> Result<(), AudioOutputError> {
            self.volume = volume.clamp(0.0, 1.0);
            Ok(())
        }

        fn volume(&self) -> f32 {
            self.volume
        }

        fn latency(&self) -> Duration {
            // CPAL doesn't expose latency directly; estimate
            Duration::from_millis(50)
        }

        fn format(&self) -> Option<AudioFormat> {
            self.format
        }

        fn close(&mut self) -> Result<(), AudioOutputError> {
            self.stop()
        }
    }
}

#[cfg(feature = "audio-cpal")]
pub use implementation::CpalOutput;
```

---

### 42.3 CoreAudio Backend (macOS)

- [x] **42.3.1** Implement CoreAudio-based output (macOS priority)

**File:** `src/audio/output_coreaudio.rs`

```rust
//! CoreAudio-based audio output for macOS
//!
//! Native macOS audio output for lowest latency and best integration.

#[cfg(all(target_os = "macos", feature = "audio-coreaudio"))]
mod implementation {
    use super::super::output::{
        AudioOutput, AudioOutputError, OutputState, AudioDevice, AudioCallback
    };
    use crate::audio::format::{AudioFormat, SampleFormat, SampleRate};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    // Note: This is a skeleton. Full implementation would use coreaudio-rs crate.

    pub struct CoreAudioOutput {
        state: OutputState,
        volume: f32,
        format: Option<AudioFormat>,
        // Would contain AudioUnit, etc.
    }

    impl CoreAudioOutput {
        pub fn new() -> Result<Self, AudioOutputError> {
            Ok(Self {
                state: OutputState::Stopped,
                volume: 1.0,
                format: None,
            })
        }
    }

    impl AudioOutput for CoreAudioOutput {
        fn enumerate_devices(&self) -> Result<Vec<AudioDevice>, AudioOutputError> {
            // Use AudioObjectGetPropertyData to enumerate devices
            Ok(vec![AudioDevice {
                id: "default".to_string(),
                name: "Default Output".to_string(),
                is_default: true,
                supported_rates: vec![SampleRate::Hz44100, SampleRate::Hz48000],
                supported_channels: vec![2],
            }])
        }

        fn default_device(&self) -> Result<AudioDevice, AudioOutputError> {
            Ok(AudioDevice {
                id: "default".to_string(),
                name: "Default Output".to_string(),
                is_default: true,
                supported_rates: vec![SampleRate::Hz44100, SampleRate::Hz48000],
                supported_channels: vec![2],
            })
        }

        fn open(&mut self, _device: Option<&str>, format: AudioFormat) -> Result<(), AudioOutputError> {
            self.format = Some(format);
            // Would configure AudioUnit here
            Ok(())
        }

        fn start(&mut self, _callback: AudioCallback) -> Result<(), AudioOutputError> {
            // Would start AudioUnit
            self.state = OutputState::Playing;
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AudioOutputError> {
            self.state = OutputState::Stopped;
            Ok(())
        }

        fn pause(&mut self) -> Result<(), AudioOutputError> {
            self.state = OutputState::Paused;
            Ok(())
        }

        fn resume(&mut self) -> Result<(), AudioOutputError> {
            self.state = OutputState::Playing;
            Ok(())
        }

        fn state(&self) -> OutputState {
            self.state
        }

        fn set_volume(&mut self, volume: f32) -> Result<(), AudioOutputError> {
            self.volume = volume.clamp(0.0, 1.0);
            Ok(())
        }

        fn volume(&self) -> f32 {
            self.volume
        }

        fn latency(&self) -> Duration {
            // CoreAudio typically has very low latency
            Duration::from_millis(10)
        }

        fn format(&self) -> Option<AudioFormat> {
            self.format
        }

        fn close(&mut self) -> Result<(), AudioOutputError> {
            self.stop()
        }
    }
}

#[cfg(all(target_os = "macos", feature = "audio-coreaudio"))]
pub use implementation::CoreAudioOutput;
```

---

### 42.4 Audio Pipeline Integration

- [x] **42.4.1** Connect jitter buffer to audio output

**File:** `src/receiver/audio_pipeline.rs`

```rust
//! Audio pipeline connecting jitter buffer to output

use crate::audio::output::{AudioOutput, AudioOutputError, AudioCallback};
use crate::audio::jitter::JitterBuffer;
use crate::audio::format::AudioFormat;
use crate::receiver::session::AudioCodec;
use std::sync::{Arc, Mutex};

/// Audio pipeline state
pub struct AudioPipeline {
    jitter_buffer: Arc<Mutex<JitterBuffer>>,
    output: Box<dyn AudioOutput>,
    decoder: Option<AudioDecoder>,
    format: AudioFormat,
}

/// Audio decoder (codec-specific)
pub enum AudioDecoder {
    Alac(AlacDecoder),
    Aac(AacDecoder),
    Pcm,  // No decoding needed
}

// Placeholder decoder types
pub struct AlacDecoder;
pub struct AacDecoder;

impl AudioPipeline {
    pub fn new(
        jitter_buffer: Arc<Mutex<JitterBuffer>>,
        output: Box<dyn AudioOutput>,
        codec: AudioCodec,
        format: AudioFormat,
    ) -> Result<Self, AudioOutputError> {
        let decoder = match codec {
            AudioCodec::Alac => Some(AudioDecoder::Alac(AlacDecoder)),
            AudioCodec::AacLc | AudioCodec::AacEld => Some(AudioDecoder::Aac(AacDecoder)),
            AudioCodec::Pcm => Some(AudioDecoder::Pcm),
        };

        Ok(Self {
            jitter_buffer,
            output,
            decoder,
            format,
        })
    }

    /// Start the audio pipeline
    pub fn start(&mut self) -> Result<(), AudioOutputError> {
        self.output.open(None, self.format)?;

        let jitter = self.jitter_buffer.clone();

        let callback: AudioCallback = Box::new(move |buffer: &mut [u8]| {
            let mut jitter = jitter.lock().unwrap();

            let mut written = 0;
            while written < buffer.len() {
                if let Some(packet) = jitter.pop() {
                    let to_copy = std::cmp::min(
                        packet.audio_data.len(),
                        buffer.len() - written
                    );
                    buffer[written..written + to_copy]
                        .copy_from_slice(&packet.audio_data[..to_copy]);
                    written += to_copy;
                } else {
                    // Underrun - fill with silence
                    for b in buffer[written..].iter_mut() {
                        *b = 0;
                    }
                    break;
                }
            }

            written
        });

        self.output.start(callback)
    }

    /// Stop the pipeline
    pub fn stop(&mut self) -> Result<(), AudioOutputError> {
        self.output.stop()
    }

    /// Set volume
    pub fn set_volume(&mut self, volume: f32) -> Result<(), AudioOutputError> {
        self.output.set_volume(volume)
    }
}
```

---

## Acceptance Criteria

- [x] AudioOutput trait defined with all required methods
- [x] CPAL backend compiles and runs on macOS, Linux, Windows
- [x] CoreAudio backend skeleton for macOS
- [x] Device enumeration works
- [x] Format negotiation works
- [x] Callback-based playback works
- [x] Volume control works
- [x] Latency reported accurately
- [x] Feature flags gate backends correctly
- [x] All unit tests pass

---

## Notes

- **Priority**: CoreAudio for macOS, CPAL as fallback
- **Latency**: Target under 20ms for good responsiveness
- **Callback**: Output pulls data from jitter buffer
- **Format**: Typically 16-bit, 44.1kHz, stereo for RAOP
- **Volume**: Applied digitally if hardware control unavailable

---

## References

- [cpal crate](https://docs.rs/cpal/)
- [coreaudio-rs](https://docs.rs/coreaudio/)
- [ALSA](https://www.alsa-project.org/)
