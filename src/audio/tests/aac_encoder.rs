use crate::audio::aac_encoder::AacEncoder;

#[test]
fn test_aac_encoding() {
    // 44.1kHz, Stereo, 64kbps
    let mut encoder = AacEncoder::new(44100, 2, 64000).expect("Failed to create encoder");

    // 1024 samples (AAC frame size usually) * 2 channels
    let input = vec![0i16; 1024 * 2];

    let output = encoder.encode(&input).expect("Encoding failed");

    // First frame might be special (silent), but should return data or empty vec
    // fdk-aac usually buffers some input.
    // We might need to feed more data to get output.

    // Feed another frame
    let output2 = encoder.encode(&input).expect("Encoding failed");

    // We expect some data eventually
    assert!(
        !output.is_empty() || !output2.is_empty(),
        "Encoder produced no output after 2 frames"
    );

    if !output.is_empty() {
        // AAC frame header + data
        println!("Output size: {}", output.len());
    }
}

#[test]
fn test_encoder_configurations() {
    // Mono
    let mut encoder = AacEncoder::new(44100, 1, 64000).expect("Mono encoder failed");
    let input = vec![0i16; 1024]; // 1 channel
    let _ = encoder.encode(&input).expect("Encoding failed");

    // Stereo, higher bitrate
    let mut encoder = AacEncoder::new(48000, 2, 128_000).expect("Stereo encoder failed");
    let input = vec![0i16; 2048]; // 2 channels
    let _ = encoder.encode(&input).expect("Encoding failed");
}

#[test]
fn test_encoder_errors() {
    // Invalid channel count
    let result = AacEncoder::new(44100, 5, 64000);
    assert!(result.is_err());
}
