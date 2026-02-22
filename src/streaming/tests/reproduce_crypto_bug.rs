#[cfg(test)]
mod tests {
    use crate::protocol::raop::RaopSessionKeys;
    use crate::protocol::rtp::raop::RaopAudioPacket;
    use crate::streaming::{RaopStreamConfig, RaopStreamer};

    #[test]
    fn test_reproduce_keystream_reuse() {
        // Setup
        let keys = RaopSessionKeys::generate().unwrap();
        let config = RaopStreamConfig::default();
        let mut streamer = RaopStreamer::new(&keys, config);

        // Encode two identical frames
        // In a proper CTR mode with continuous keystream, C1 and C2 should be different
        // because the keystream advances.
        // In the buggy implementation (resetting cipher), C1 and C2 will be identical.

        let audio_data = vec![0xAB; 100]; // Arbitrary data

        let packet1 = streamer.encode_frame(&audio_data);
        let packet2 = streamer.encode_frame(&audio_data);

        // Extract payloads (skip header)
        let payload1 = &packet1[RaopAudioPacket::HEADER_SIZE..];
        let payload2 = &packet2[RaopAudioPacket::HEADER_SIZE..];

        // Verification
        // If they are equal, the keystream was reused (BUG)
        // If they are different, the keystream advanced (CORRECT)
        assert_ne!(
            payload1, payload2,
            "Critical security flaw: Keystream reuse detected! Consecutive packets encrypted with \
             same keystream."
        );
    }
}
