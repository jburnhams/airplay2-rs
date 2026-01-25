use super::{RaopStreamConfig, RaopStreamer};
use crate::protocol::raop::RaopSessionKeys;

fn create_test_keys() -> RaopSessionKeys {
    RaopSessionKeys {
        aes_key: [0x42; 16],
        aes_iv: [0x00; 16],
        encrypted_key: vec![], // Not used for streaming, only for SDP
    }
}

#[test]
fn test_streaming_sequence() {
    // Create mock session keys
    let keys = create_test_keys();
    let config = RaopStreamConfig::default();

    let mut streamer = RaopStreamer::new(keys, config);

    // Simulate streaming audio frames
    let frame = vec![0u8; 352 * 4]; // 352 samples * 4 bytes (16-bit stereo)

    let packet1 = streamer.encode_frame(&frame);
    let packet2 = streamer.encode_frame(&frame);
    let _packet3 = streamer.encode_frame(&frame);

    // Check sequence numbers
    assert_eq!(streamer.sequence(), 3);

    // Check timestamp progression
    assert_eq!(streamer.timestamp(), 352 * 3);

    // First packet should have marker bit (byte 1, bit 7)
    // RTP header: [V=2 P=0 X=0 CC=0] [M PT]
    // PT=0x60 (96). With Marker: 0xE0 (224).
    // Without marker: 0x60 (96).
    // byte 1 is (payload_type | (marker << 7))
    assert_eq!(packet1[1] & 0x80, 0x80);
    // Subsequent packets should not
    assert_eq!(packet2[1] & 0x80, 0x00);
}

#[test]
fn test_retransmit_buffer() {
    let keys = create_test_keys();
    let config = RaopStreamConfig::default();
    let mut streamer = RaopStreamer::new(keys, config);
    let frame = vec![0u8; 352 * 4];

    streamer.encode_frame(&frame); // seq 0
    streamer.encode_frame(&frame); // seq 1
    streamer.encode_frame(&frame); // seq 2

    let retransmits = streamer.handle_retransmit(0, 2);
    assert_eq!(retransmits.len(), 2);
    // Check RTP header sequence numbers in retransmit packets
    // Retransmit packet format: [Header 4 bytes] [Original RTP Header minus 4 bytes]
    // Header: 0x80 0xD6 Seq(2)
    // Sequence number is at offset 2 and 3.
    assert_eq!(retransmits[0][2], 0); // seq 0 high
    assert_eq!(retransmits[0][3], 0); // seq 0 low

    assert_eq!(retransmits[1][2], 0); // seq 1 high
    assert_eq!(retransmits[1][3], 1); // seq 1 low
}

#[test]
fn test_sync_packet() {
    let keys = create_test_keys();
    let config = RaopStreamConfig::default();
    let mut streamer = RaopStreamer::new(keys, config);

    // Initially sync should be sent (or not? logic depends on should_send_sync)
    // should_send_sync checks elapsed time.
    // We can force create_sync_packet
    let packet = streamer.create_sync_packet();
    assert_eq!(packet.len(), 20); // SyncPacket::SIZE
}
