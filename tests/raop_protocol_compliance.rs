//! RAOP protocol compliance tests
//!
//! These tests verify that the implementation conforms to the
//! RAOP protocol specification.

use airplay2::protocol::raop::session::*;
use airplay2::protocol::sdp::*;

/// Test RTSP request format compliance
#[cfg(feature = "raop")]
mod rtsp_compliance {
    use super::*;

    #[test]
    fn test_options_request_format() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);
        let request = session.options_request();

        // Verify via encoded string
        let bytes = request.encode();
        let encoded = String::from_utf8_lossy(&bytes);
        assert!(encoded.contains("RTSP/1.0"));

        assert!(request.headers.get("CSeq").is_some());
        assert!(request.headers.get("User-Agent").is_some());
        assert!(request.headers.get("Apple-Challenge").is_some());
    }

    #[test]
    fn test_announce_sdp_format() {
        let sdp = create_raop_announce_sdp(
            "1234567890",
            "192.168.1.100",
            "192.168.1.50",
            "encrypted_key",
            "init_vector",
        );

        // Must start with v=0
        assert!(sdp.starts_with("v=0"));

        // Must have origin line
        assert!(sdp.contains("o=iTunes"));

        // Must have connection line
        assert!(sdp.contains("c=IN IP"));

        // Must have media line
        assert!(sdp.contains("m=audio"));

        // Must have encryption attributes
        assert!(sdp.contains("a=rsaaeskey:"));
        assert!(sdp.contains("a=aesiv:"));
    }

    #[test]
    fn test_setup_transport_format() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);
        let request = session.setup_request(6001, 6002);

        // Check fields
        let transport = request
            .headers
            .get("Transport")
            .expect("Transport header missing");

        // Must specify RTP/AVP/UDP
        assert!(transport.contains("RTP/AVP/UDP"));

        // Must specify mode=record
        assert!(transport.contains("mode=record"));

        // Must specify ports
        assert!(transport.contains("control_port=6001"));
        assert!(transport.contains("timing_port=6002"));
    }

    #[test]
    fn test_cseq_increments() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);

        let r1 = session.options_request();
        let r2 = session.options_request();
        let r3 = session.setup_request(6001, 6002);

        assert_eq!(r1.headers.cseq(), Some(1));
        assert_eq!(r2.headers.cseq(), Some(2));
        assert_eq!(r3.headers.cseq(), Some(3));
    }
}

/// Test RTP packet format compliance
#[cfg(feature = "raop")]
mod rtp_compliance {
    use airplay2::protocol::rtp::raop::*;

    #[test]
    fn test_audio_packet_header() {
        let packet = RaopAudioPacket::new(100, 44100, 0x12345678, vec![0; 100]);
        let encoded = packet.encode();

        // Version must be 2 (bits 6-7 of byte 0)
        assert_eq!((encoded[0] >> 6) & 0x03, 2);

        // Payload type must be 0x60 (realtime) or 0x61 (buffered)
        assert_eq!(encoded[1] & 0x7F, 0x60);

        // Sequence number (bytes 2-3, big-endian)
        assert_eq!(u16::from_be_bytes([encoded[2], encoded[3]]), 100);

        // Timestamp (bytes 4-7, big-endian)
        assert_eq!(
            u32::from_be_bytes([encoded[4], encoded[5], encoded[6], encoded[7]]),
            44100
        );
    }

    #[test]
    fn test_marker_bit_on_first_packet() {
        let packet = RaopAudioPacket::new(0, 0, 0, vec![]).with_marker();
        let encoded = packet.encode();

        // Marker bit is bit 7 of byte 1
        assert_eq!(encoded[1] & 0x80, 0x80);
    }

    #[test]
    fn test_sync_packet_format() {
        use airplay2::protocol::rtp::NtpTimestamp;

        let ntp = NtpTimestamp {
            seconds: 0x12345678,
            fraction: 0x9ABCDEF0,
        };
        let packet = SyncPacket::new(1000, ntp, 1352, true);
        let encoded = packet.encode();

        // Payload type must be 0x54
        assert_eq!(encoded[1] & 0x7F, 0x54);

        // Extension bit set for first sync
        assert_eq!(encoded[0] & 0x10, 0x10);

        // Total size must be 20 bytes
        assert_eq!(encoded.len(), 20);
    }
}

/// Test timing protocol compliance
#[cfg(feature = "raop")]
mod timing_compliance {
    use airplay2::protocol::rtp::raop_timing::*;

    #[test]
    fn test_timing_request_format() {
        let request = RaopTimingRequest::new();
        let encoded = request.encode(1);

        // Must be 32 bytes
        assert_eq!(encoded.len(), 32);

        // Payload type must be 0x52
        assert_eq!(encoded[1] & 0x7F, 0x52);

        // Marker bit should be set
        assert_eq!(encoded[1] & 0x80, 0x80);
    }
}

/// Test DMAP encoding compliance
#[cfg(feature = "raop")]
mod dmap_compliance {
    use airplay2::protocol::daap::{DmapEncoder, DmapTag};

    #[test]
    fn test_dmap_tag_codes() {
        // Verify tag codes match DAAP specification
        assert_eq!(DmapTag::ItemName.code(), *b"minm");
        assert_eq!(DmapTag::SongArtist.code(), *b"asar");
        assert_eq!(DmapTag::SongAlbum.code(), *b"asal");
    }

    #[test]
    fn test_dmap_length_encoding() {
        let mut encoder = DmapEncoder::new();
        encoder.string(DmapTag::ItemName, "Test");
        let data = encoder.finish();

        // Tag (4) + Length (4) + "Test" (4) = 12 bytes
        assert_eq!(data.len(), 12);

        // Length field (bytes 4-7) should be 4
        assert_eq!(u32::from_be_bytes([data[4], data[5], data[6], data[7]]), 4);
    }
}
