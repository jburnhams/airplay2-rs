//! Audio output abstraction
//!
//! Platform-agnostic trait for audio playback with implementations
//! for CoreAudio, CPAL, ALSA, etc.

use crate::audio::format::{AudioFormat, SampleRate};
use std::time::Duration;

/// Errors from audio output
#[derive(Debug, thiserror::Error)]
pub enum AudioOutputError {
    /// Device not found
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    /// Format not supported
    #[error("Format not supported: {0:?}")]
    FormatNotSupported(AudioFormat),

    /// Stream error
    #[error("Stream error: {0}")]
    StreamError(String),

    /// Generic device error
    #[error("Device error: {0}")]
    DeviceError(String),

    /// Buffer underrun
    #[error("Buffer underrun")]
    Underrun,

    /// Output closed
    #[error("Output closed")]
    Closed,
}

/// Audio output state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputState {
    /// Stopped
    Stopped,
    /// Playing
    Playing,
    /// Paused
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
    fn open(&mut self, device: Option<&str>, format: AudioFormat) -> Result<(), AudioOutputError>;

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
    #[cfg(all(feature = "audio-coreaudio", target_os = "macos"))]
    {
        return Ok(Box::new(super::output_coreaudio::CoreAudioOutput::new()?));
    }

    #[cfg(feature = "audio-cpal")]
    {
        #[allow(unreachable_code)]
        return Ok(Box::new(super::output_cpal::CpalOutput::new()?));
    }

    #[cfg(all(
        not(any(feature = "audio-coreaudio", feature = "audio-cpal")),
        feature = "audio-alsa"
    ))]
    {
        // return Ok(Box::new(super::output_alsa::AlsaOutput::new()?));
        // Placeholder as alsa is not implemented yet
        return Err(AudioOutputError::DeviceError("Alsa not implemented".into()));
    }

    #[cfg(not(any(
        feature = "audio-coreaudio",
        feature = "audio-cpal",
        feature = "audio-alsa"
    )))]
    {
        Err(AudioOutputError::DeviceError(
            "No audio backend enabled. Enable audio-coreaudio, audio-cpal, or audio-alsa feature."
                .into(),
        ))
    }
}
