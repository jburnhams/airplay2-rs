use crate::protocol::rtp::RtpCodec;

#[test]
fn test_chacha_roundtrip() {
    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let mut decoder = RtpCodec::new(0);
    decoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0xAAu8; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&audio, &mut packet).unwrap();

    let decoded_packet = decoder.decode_audio(&packet).unwrap();
    assert_eq!(decoded_packet.payload, audio);
}

#[test]
fn test_chacha_nonce_increment() {
    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0u8; frame_size];

    // Packet 1
    let mut packet1 = Vec::new();
    encoder.encode_audio(&audio, &mut packet1).unwrap();

    // Packet 2
    let mut packet2 = Vec::new();
    encoder.encode_audio(&audio, &mut packet2).unwrap();

    // Extract nonces (last 8 bytes)
    let nonce1 = &packet1[packet1.len() - 8..];
    let nonce2 = &packet2[packet2.len() - 8..];

    assert_eq!(nonce1, &[0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(nonce2, &[1, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn test_chacha_tag_validation_failure() {
    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let mut decoder = RtpCodec::new(0);
    decoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0u8; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&audio, &mut packet).unwrap();

    // Tamper with tag (16 bytes before last 8 bytes)
    let tag_idx = packet.len() - 8 - 16;
    packet[tag_idx] ^= 0xFF;

    let result = decoder.decode_audio(&packet);
    assert!(result.is_err());
}

#[test]
fn test_chacha_aad_validation_failure() {
    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let mut decoder = RtpCodec::new(0);
    decoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0u8; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&audio, &mut packet).unwrap();

    // Tamper with header (AAD includes timestamp at offset 4)
    packet[4] ^= 0xFF;

    let result = decoder.decode_audio(&packet);
    assert!(result.is_err());
}

#[test]
fn test_chacha_packet_structure() {
    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0u8; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&audio, &mut packet).unwrap();

    // Structure: Header(12) + Encrypted(len) + Tag(16) + Nonce(8)
    let expected_len = 12 + audio.len() + 16 + 8;
    assert_eq!(packet.len(), expected_len);
}
