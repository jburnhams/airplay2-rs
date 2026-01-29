use super::*;

#[test]
fn test_codec_sequence_increment() {
    let mut codec = RtpCodec::new(0x12345678);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0u8; frame_size];
    let mut packet = Vec::new();

    codec.encode_audio(&audio, &mut packet).unwrap();
    assert_eq!(codec.sequence(), 1);

    codec.encode_audio(&audio, &mut packet).unwrap();
    assert_eq!(codec.sequence(), 2);
}

#[test]
fn test_codec_timestamp_increment() {
    let mut codec = RtpCodec::new(0x12345678);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0u8; frame_size];
    let mut packet = Vec::new();

    codec.encode_audio(&audio, &mut packet).unwrap();
    assert_eq!(codec.timestamp(), 352);

    codec.encode_audio(&audio, &mut packet).unwrap();
    assert_eq!(codec.timestamp(), 704);
}

#[test]
fn test_codec_invalid_audio_size() {
    let mut codec = RtpCodec::new(0);
    let audio = vec![0u8; 100]; // Wrong size
    let mut packet = Vec::new();

    let result = codec.encode_audio(&audio, &mut packet);
    assert!(matches!(result, Err(RtpCodecError::InvalidAudioSize(100))));
}

#[test]
fn test_codec_encode_multiple_frames() {
    let mut codec = RtpCodec::new(0);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0u8; frame_size * 3]; // 3 frames

    let packets = codec.encode_audio_frames(&audio).unwrap();

    assert_eq!(packets.len(), 3);
    assert_eq!(codec.sequence(), 3);
}

#[test]
fn test_codec_with_encryption() {
    let mut codec = RtpCodec::new(0);
    let key = [0x42u8; 16];
    let iv = [0x00u8; 16];
    codec.set_encryption(key, iv);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0xAA; frame_size];
    let mut packet = Vec::new();

    codec.encode_audio(&audio, &mut packet).unwrap();

    // Encrypted payload should differ from original
    let decoded = RtpPacket::decode(&packet).unwrap();
    assert_ne!(decoded.payload, audio);
}

#[test]
fn test_codec_encrypt_decrypt_roundtrip() {
    let key = [0x42u8; 16];
    let iv = [0x00u8; 16];

    let mut encoder = RtpCodec::new(0x12345678);
    encoder.set_encryption(key, iv);

    let _decoder = RtpCodec::new(0x12345678);
    // Note: decoder needs same keys for decryption

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let original = vec![0xAA; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&original, &mut packet).unwrap();

    // Create decoder with same keys
    let mut decoder = RtpCodec::new(0);
    decoder.set_encryption(key, iv);

    let decoded = decoder.decode_audio(&packet).unwrap();
    assert_eq!(decoded.payload, original);
}

#[test]
fn test_packet_builder() {
    let builder = AudioPacketBuilder::new(0x1234);
    let packets = builder.add_audio(&vec![0u8; 352 * 4]).unwrap().build();

    assert_eq!(packets.len(), 1);
}
