use crate::protocol::rtp::{AudioPacketBuilder, RtpCodec, RtpCodecError, RtpPacket};

#[test]
fn test_codec_sequence_increment() {
    let mut codec = RtpCodec::new(0x1234_5678);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0u8; frame_size];
    let mut packet = Vec::new();

    codec.encode_audio(&audio, &mut packet).unwrap();
    assert_eq!(codec.sequence(), 1);

    packet.clear();
    codec.encode_audio(&audio, &mut packet).unwrap();
    assert_eq!(codec.sequence(), 2);
}

#[test]
fn test_codec_timestamp_increment() {
    let mut codec = RtpCodec::new(0x1234_5678);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let audio = vec![0u8; frame_size];
    let mut packet = Vec::new();

    codec.encode_audio(&audio, &mut packet).unwrap();
    assert_eq!(codec.timestamp(), 352);

    packet.clear();
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

    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_encryption(key, iv);

    let _decoder = RtpCodec::new(0x1234_5678);
    // Note: decoder needs same keys for decryption

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let original = vec![0xAA; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&original, &mut packet).unwrap();

    // Create decoder with same keys
    let mut rtp_decoder = RtpCodec::new(0);
    rtp_decoder.set_encryption(key, iv);

    let decoded = rtp_decoder.decode_audio(&packet).unwrap();
    assert_eq!(decoded.payload, original);
}

#[test]
fn test_packet_builder() {
    let builder = AudioPacketBuilder::new(0x1234);
    let packets = builder.add_audio(&vec![0u8; 352 * 4]).unwrap().build();

    assert_eq!(packets.len(), 1);
}

#[test]
fn test_chacha_encrypt_decrypt_roundtrip() {
    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let original = vec![0xAA; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&original, &mut packet).unwrap();

    let mut decoder = RtpCodec::new(0);
    decoder.set_chacha_encryption(key);

    let packet_decoded = decoder.decode_audio(&packet).unwrap();
    assert_eq!(packet_decoded.payload, original);
    assert_eq!(packet_decoded.header.sequence, 0);
    assert_eq!(packet_decoded.header.timestamp, 0);
    assert_eq!(packet_decoded.header.ssrc, 0x1234_5678);
}

#[test]
fn test_chacha_packet_structure() {
    use crate::protocol::rtp::RtpHeader;

    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let original = vec![0xAA; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&original, &mut packet).unwrap();

    // Check size: Header (12) + Payload + Tag (16) + Nonce (8)
    assert_eq!(
        packet.len(),
        RtpHeader::SIZE + original.len() + 16 + 8
    );

    // Verify nonce is at the end
    let nonce_bytes = &packet[packet.len() - 8..];
    assert_eq!(nonce_bytes, &[0, 0, 0, 0, 0, 0, 0, 0]); // First nonce is 0
}

#[test]
fn test_chacha_nonce_increment() {
    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let original = vec![0xAA; frame_size];

    // Packet 1
    let mut packet1 = Vec::new();
    encoder.encode_audio(&original, &mut packet1).unwrap();
    let nonce1 = &packet1[packet1.len() - 8..];
    assert_eq!(nonce1, &[0, 0, 0, 0, 0, 0, 0, 0]);

    // Packet 2
    let mut packet2 = Vec::new();
    encoder.encode_audio(&original, &mut packet2).unwrap();
    let nonce2 = &packet2[packet2.len() - 8..];
    assert_eq!(nonce2, &[1, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn test_chacha_tamper_tag() {
    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let original = vec![0xAA; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&original, &mut packet).unwrap();

    // Tamper with tag (last 8 bytes is nonce, 16 bytes before that is tag)
    let tag_offset = packet.len() - 8 - 16;
    packet[tag_offset] ^= 0xFF;

    let mut decoder = RtpCodec::new(0);
    decoder.set_chacha_encryption(key);

    let result = decoder.decode_audio(&packet);
    assert!(matches!(result, Err(RtpCodecError::DecryptionFailed(_))));
}

#[test]
fn test_chacha_tamper_payload() {
    use crate::protocol::rtp::RtpHeader;

    let key = [0x42u8; 32];
    let mut encoder = RtpCodec::new(0x1234_5678);
    encoder.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
    let original = vec![0xAA; frame_size];
    let mut packet = Vec::new();

    encoder.encode_audio(&original, &mut packet).unwrap();

    // Tamper with payload
    packet[RtpHeader::SIZE] ^= 0xFF;

    let mut decoder = RtpCodec::new(0);
    decoder.set_chacha_encryption(key);

    let result = decoder.decode_audio(&packet);
    assert!(matches!(result, Err(RtpCodecError::DecryptionFailed(_))));
}
