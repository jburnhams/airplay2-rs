use airplay2::protocol::raop::RaopSessionKeys;
use airplay2::streaming::{RaopStreamConfig, RaopStreamer};

#[test]
fn test_streaming_integration() {
    // We need to generate keys. This requires crypto primitives to be working.
    // Ensure that the static key is available or mocked if necessary.
    // In this codebase, AppleRsaPublicKey seems to have the key embedded.
    let keys = RaopSessionKeys::generate().expect("Failed to generate keys");
    let config = RaopStreamConfig::default();

    let mut streamer = RaopStreamer::new(&keys, config);

    // Encode some frames
    let frame = vec![0u8; 352 * 4];

    // First packet
    let packet1 = streamer.encode_frame(&frame);
    // Header (12) + Payload
    assert_eq!(packet1.len(), 12 + frame.len());

    // Check marker bit on first packet (PT=0x60 | 0x80 = 0xE0)
    assert_eq!(packet1[1] & 0x80, 0x80);

    // Second packet
    let packet2 = streamer.encode_frame(&frame);
    assert_eq!(packet2[1] & 0x80, 0x00);

    assert_eq!(streamer.sequence(), 2);

    // Test retransmission
    let retransmits = streamer.handle_retransmit(0, 1);
    assert_eq!(retransmits.len(), 1);
}
