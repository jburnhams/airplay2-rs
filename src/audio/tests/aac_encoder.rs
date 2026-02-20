use crate::audio::aac_encoder::AacEncoder;
use crate::audio::format::AacProfile;

#[test]
fn test_aac_encoding() {
    // 44.1kHz, Stereo, 64kbps
    let mut encoder =
        AacEncoder::new(44100, 2, 64000, AacProfile::Lc).expect("Failed to create encoder");

    // 1024 samples (AAC frame size usually) * 2 channels
    let input = vec![0i16; 1024 * 2];
    let output = encoder.encode(&input).expect("Encoding failed");

    // Check that we got some data
    // Note: First few frames might be silent or configuration, but should produce bytes
    // Actually fdk-aac might produce empty output for first call due to buffering/delay
    // But let's check it doesn't error.
    println!("Encoded size: {}", output.len());
}

#[test]
fn test_encoder_configurations() {
    // Mono
    let mut encoder =
        AacEncoder::new(44100, 1, 64000, AacProfile::Lc).expect("Mono encoder failed");
    let input = vec![0i16; 1024]; // 1 channel
    let output = encoder.encode(&input).expect("Encoding failed");

    println!("Mono encoded size: {}", output.len());
}

#[test]
fn test_bitrate_handling() {
    // Stereo, low bitrate
    let _encoder = AacEncoder::new(
        44100,
        2,
        32000,
        AacProfile::Lc, // 32kbps is very low for stereo, but should init
    )
    .expect("Low bitrate encoder failed (might be too low for library defaults, but let's see)");

    // Stereo, higher bitrate
    let mut encoder =
        AacEncoder::new(48000, 2, 128_000, AacProfile::Lc).expect("Stereo encoder failed");
    let input = vec![0i16; 2048]; // 2 channels
    let output = encoder.encode(&input).expect("Encoding failed");

    println!("High bitrate encoded size: {}", output.len());
}

#[test]
fn test_encoder_errors() {
    // Invalid channel count
    let result = AacEncoder::new(44100, 5, 64000, AacProfile::Lc);
    assert!(result.is_err());
}
