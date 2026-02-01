use crate::protocol::rtsp::{Headers, Method, RtspRequest};
use crate::receiver::announce_handler::*;

#[test]
fn test_process_announce_valid() {
    let sdp = r"v=0
o=- 0 0 IN IP4 127.0.0.1
s=AirTunes
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
";
    let request = RtspRequest {
        method: Method::Announce,
        uri: "rtsp://localhost/stream".to_string(),
        headers: Headers::new(),
        body: sdp.as_bytes().to_vec(),
    };

    let params = process_announce(&request, None).unwrap();
    assert_eq!(params.sample_rate, 44100);
}

#[test]
fn test_process_announce_empty_body() {
    let request = RtspRequest {
        method: Method::Announce,
        uri: "rtsp://localhost/stream".to_string(),
        headers: Headers::new(),
        body: Vec::new(),
    };

    let err = process_announce(&request, None).unwrap_err();
    assert!(matches!(err, AnnounceError::EmptyBody));
}

#[test]
fn test_process_announce_invalid_utf8() {
    let request = RtspRequest {
        method: Method::Announce,
        uri: "rtsp://localhost/stream".to_string(),
        headers: Headers::new(),
        body: vec![0xFF, 0xFE, 0xFD], // Invalid UTF-8 sequence
    };

    let err = process_announce(&request, None).unwrap_err();
    assert!(matches!(err, AnnounceError::InvalidUtf8));
}

#[test]
fn test_process_announce_sdp_error() {
    let request = RtspRequest {
        method: Method::Announce,
        uri: "rtsp://localhost/stream".to_string(),
        headers: Headers::new(),
        body: "invalid sdp".as_bytes().to_vec(),
    };

    let err = process_announce(&request, None).unwrap_err();
    assert!(matches!(err, AnnounceError::SdpParse(_)));
}
