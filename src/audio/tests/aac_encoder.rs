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
    let mut encoder = AacEncoder::new(44100, 1, 64000, AacProfile::Lc).expect("Mono encoder failed");
    let input = vec![0i16; 1024]; // 1 channel
    let output = encoder.encode(&input).expect("Encoding failed");

    // fdk-aac may buffer the first frame
    let output2 = if output.is_empty() {
        encoder.encode(&input).expect("Encoding failed")
    } else {
        Vec::new()
    };

    assert!(
        !output.is_empty() || !output2.is_empty(),
        "Mono encoder produced no output after 2 frames"
    );

    // Stereo, higher bitrate
    let mut encoder =
        AacEncoder::new(48000, 2, 128_000, AacProfile::Lc).expect("Stereo encoder failed");
    let input = vec![0i16; 2048]; // 2 channels
    let output = encoder.encode(&input).expect("Encoding failed");

    // fdk-aac may buffer the first frame
    let output2 = if output.is_empty() {
        encoder.encode(&input).expect("Encoding failed")
    } else {
        Vec::new()
    };

    assert!(
        !output.is_empty() || !output2.is_empty(),
        "Stereo encoder produced no output after 2 frames"
    );
}

#[test]
fn test_encoder_errors() {
    // Invalid channel count
    let result = AacEncoder::new(44100, 5, 64000, AacProfile::Lc);
    assert!(result.is_err());
}
