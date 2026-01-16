use airplay2::protocol::rtp::{RtpCodec, RtpPacket};

#[test]
fn test_audio_streaming_simulation() {
    let mut codec = RtpCodec::new(0xDEADBEEF);

    // Simulate 1 second of audio at 44.1kHz
    // 44100 samples / 352 samples per packet ≈ 125 packets
    let total_samples = 44100;
    let total_bytes = total_samples * 4;

    let audio_data = vec![0u8; total_bytes];
    let packets = codec.encode_audio_frames(&audio_data).unwrap();

    assert!((packets.len() as i32 - 125).abs() <= 1);

    // Verify sequence numbers are continuous
    for (i, packet_data) in packets.iter().enumerate() {
        let packet = RtpPacket::decode(packet_data).unwrap();
        assert_eq!(packet.header.sequence, i as u16);
    }
}
