use airplay2::protocol::rtp::RtpCodec;

#[test]
fn test_ap2_streaming_integration_chacha() {
    // 1. Setup ChaCha20Poly1305 key
    let key = [0x55u8; 32];

    // 2. Setup Sender
    let mut sender = RtpCodec::new(0x1122_3344);
    sender.set_chacha_encryption(key);

    // 3. Setup Receiver
    // Note: Receiver needs same key. SSRC in constructor doesn't matter for decoding logic
    // unless we validate it, but RtpCodec decode doesn't validate SSRC against internal state.
    let mut receiver = RtpCodec::new(0);
    receiver.set_chacha_encryption(key);

    let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;

    // 4. Encode/Decode loop for 10 packets
    for i in 0..10 {
        let expected_sequence = i as u16;
        let expected_timestamp = (i as u32) * RtpCodec::FRAMES_PER_PACKET;

        // Create dummy audio frame (e.g. counter values to be unique)
        let audio: Vec<u8> = (0..frame_size).map(|b| ((b + i) % 256) as u8).collect();

        let mut packet_data = Vec::new();
        sender
            .encode_audio(&audio, &mut packet_data)
            .expect("Encoding failed");

        // Verify sender updated state
        assert_eq!(sender.sequence(), expected_sequence + 1);
        assert_eq!(
            sender.timestamp(),
            expected_timestamp + RtpCodec::FRAMES_PER_PACKET
        );

        // Receiver process
        // In real network, packet_data would be sent over UDP. Here we just pass the bytes.
        let decoded_packet = receiver
            .decode_audio(&packet_data)
            .expect("Decoding failed");

        // Verify headers
        assert_eq!(
            decoded_packet.header.sequence, expected_sequence,
            "Sequence mismatch at packet {}",
            i
        );
        assert_eq!(
            decoded_packet.header.timestamp, expected_timestamp,
            "Timestamp mismatch at packet {}",
            i
        );
        assert_eq!(decoded_packet.header.ssrc, 0x1122_3344);

        // Verify payload
        assert_eq!(
            decoded_packet.payload, audio,
            "Payload mismatch at packet {}",
            i
        );
    }
}
