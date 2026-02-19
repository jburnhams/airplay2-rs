use proptest::prelude::*;

use crate::protocol::rtsp::codec::RtspCodec;

proptest! {
    // Fuzz with random byte sequences
    #[test]
    fn test_codec_no_panic_on_random_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let mut codec = RtspCodec::new();
        // Ignore errors, just check for panics
        let _ = codec.feed(&bytes);
        let _ = codec.decode();
    }

    // Fuzz with random ASCII strings (more likely to hit parser logic)
    #[test]
    fn test_codec_no_panic_on_random_ascii(s in "[ -~]{0,1024}") {
        let mut codec = RtspCodec::new();
        let _ = codec.feed(s.as_bytes());
        let _ = codec.decode();
    }

    // Fuzz with random RTSP response-like strings
    #[test]
    fn test_codec_no_panic_on_random_rtsp_response(
        version in "RTSP/1\\.0",
        status in 100..600u16,
        reason in "[a-zA-Z ]+",
        header_name in "[A-Za-z-]+",
        header_value in "[ -~]+",
        body in ".*"
    ) {
        let response = format!("{} {} {}\r\n{}: {}\r\nContent-Length: {}\r\n\r\n{}",
            version, status, reason, header_name, header_value, body.len(), body);

        let mut codec = RtspCodec::new();
        // We expect this might actually parse correctly if the format is right
        let _ = codec.feed(response.as_bytes());
        let _ = codec.decode();
    }
}
