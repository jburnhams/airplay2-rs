use proptest::prelude::*;

use crate::protocol::rtp::{PayloadType, RtpHeader};

proptest! {
    #[test]
    fn test_payload_type_any_byte(b in 0u8..=255) {
        // Should not panic
        let _ = PayloadType::from_byte(b);
    }

    #[test]
    fn test_header_decode_any_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..100)) {
        // Should not panic, return either Ok or Err
        let _ = RtpHeader::decode(&bytes);
    }

    #[test]
    fn test_header_encode_decode_roundtrip(
        sequence in any::<u16>(),
        timestamp in any::<u32>(),
        ssrc in any::<u32>(),
        buffered in any::<bool>()
    ) {
        let header = RtpHeader::new_audio(sequence, timestamp, ssrc, buffered);
        let encoded = header.encode();
        let decoded = RtpHeader::decode(&encoded).expect("Decode failed");

        prop_assert_eq!(decoded.version, 2);
        prop_assert_eq!(decoded.sequence, sequence);
        prop_assert_eq!(decoded.timestamp, timestamp);
        prop_assert_eq!(decoded.ssrc, ssrc);
        prop_assert!(decoded.marker); // new_audio sets marker to true

        let expected_pt = if buffered { PayloadType::AudioBuffered } else { PayloadType::AudioRealtime };
        prop_assert_eq!(decoded.payload_type, expected_pt);
    }
}
