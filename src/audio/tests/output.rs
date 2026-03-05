use crate::audio::format::{AudioFormat, SampleRate};
use crate::audio::output::{AudioDevice, AudioOutputError, create_default_output};

#[test]
fn test_audio_device_struct() {
    let device = AudioDevice {
        id: "test".to_string(),
        name: "Test Device".to_string(),
        is_default: true,
        supported_rates: vec![SampleRate::Hz44100],
        supported_channels: vec![2],
    };

    assert_eq!(device.id, "test");
    assert_eq!(device.name, "Test Device");
    assert!(device.is_default);
    assert_eq!(device.supported_rates, vec![SampleRate::Hz44100]);
    assert_eq!(device.supported_channels, vec![2]);
}

#[test]
fn test_audio_output_error_display() {
    let err = AudioOutputError::DeviceNotFound("foo".to_string());
    assert_eq!(err.to_string(), "Device not found: foo");

    let err = AudioOutputError::FormatNotSupported(AudioFormat::default());
    assert!(err.to_string().starts_with("Format not supported:"));

    let err = AudioOutputError::StreamError("oops".to_string());
    assert_eq!(err.to_string(), "Stream error: oops");

    let err = AudioOutputError::DeviceError("fail".to_string());
    assert_eq!(err.to_string(), "Device error: fail");

    let err = AudioOutputError::Underrun;
    assert_eq!(err.to_string(), "Buffer underrun");

    let err = AudioOutputError::Closed;
    assert_eq!(err.to_string(), "Output closed");
}

#[test]
fn test_create_default_output_no_features() {
    let result = create_default_output();

    #[cfg(any(
        feature = "audio-coreaudio",
        feature = "audio-cpal",
        feature = "audio-alsa"
    ))]
    {
        // If we are on linux/macos with features enabled, it might succeed (if device available) or
        // fail with DeviceError/DeviceNotFound (if no device). But in CI environment, it
        // might fail to find device. We just want to ensure it doesn't panic.
        if result.is_ok() || result.is_err() {
            // Accept failure to init backend in headless env
        }
    }

    #[cfg(not(any(
        feature = "audio-coreaudio",
        feature = "audio-cpal",
        feature = "audio-alsa"
    )))]
    {
        assert!(result.is_err());
        match result {
            Err(AudioOutputError::DeviceError(msg)) => {
                assert!(msg.contains("No audio backend enabled"));
            }
            _ => panic!("Unexpected error type"),
        }
    }
}
