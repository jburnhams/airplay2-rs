use crate::protocol::rtsp::StatusCode;
use crate::protocol::rtsp::codec::{RtspCodec, RtspCodecError};

#[test]
fn test_decode_simple_response() {
    let mut codec = RtspCodec::new();

    codec
        .feed(
            b"RTSP/1.0 200 OK\r\n\
                 CSeq: 1\r\n\
                 \r\n",
        )
        .unwrap();

    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.version, "RTSP/1.0");
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.reason, "OK");
    assert_eq!(response.cseq(), Some(1));
    assert!(response.body.is_empty());
}

#[test]
fn test_decode_response_with_body() {
    let mut codec = RtspCodec::new();

    codec
        .feed(
            b"RTSP/1.0 200 OK\r\n\
                 CSeq: 2\r\n\
                 Content-Length: 5\r\n\
                 \r\n\
                 hello",
        )
        .unwrap();

    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.body, b"hello");
}

#[test]
fn test_decode_incremental() {
    let mut codec = RtspCodec::new();

    // Feed partial data
    codec.feed(b"RTSP/1.0 200 ").unwrap();
    assert!(codec.decode().unwrap().is_none());

    codec.feed(b"OK\r\n").unwrap();
    assert!(codec.decode().unwrap().is_none());

    codec.feed(b"CSeq: 1\r\n\r\n").unwrap();
    assert!(codec.decode().unwrap().is_some());
}

#[test]
fn test_decode_multiple_responses() {
    let mut codec = RtspCodec::new();

    codec
        .feed(
            b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n\
                 RTSP/1.0 200 OK\r\nCSeq: 2\r\n\r\n",
        )
        .unwrap();

    let r1 = codec.decode().unwrap().unwrap();
    assert_eq!(r1.cseq(), Some(1));

    let r2 = codec.decode().unwrap().unwrap();
    assert_eq!(r2.cseq(), Some(2));

    assert!(codec.decode().unwrap().is_none());
}

#[test]
fn test_decode_invalid_status_line() {
    let mut codec = RtspCodec::new();

    codec.feed(b"INVALID LINE\r\n\r\n").unwrap();

    let result = codec.decode();
    assert!(matches!(result, Err(RtspCodecError::InvalidStatusLine(_))));
}

#[test]
fn test_status_code_checks() {
    assert!(StatusCode::OK.is_success());
    assert!(!StatusCode::OK.is_client_error());

    assert!(StatusCode::NOT_FOUND.is_client_error());
    assert!(!StatusCode::NOT_FOUND.is_success());

    assert!(StatusCode::INTERNAL_ERROR.is_server_error());
}

#[test]
fn test_max_size_limit() {
    let mut codec = RtspCodec::new().with_max_size(100);

    let result = codec.feed(&[0u8; 200]);

    assert!(matches!(
        result,
        Err(RtspCodecError::ResponseTooLarge { .. })
    ));
}

#[test]
fn test_decode_byte_by_byte() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n";

    let mut response = None;
    for byte in data {
        codec.feed(&[*byte]).unwrap();
        if let Some(r) = codec.decode().unwrap() {
            response = Some(r);
            break;
        }
    }

    assert!(response.is_some());
    assert_eq!(response.unwrap().cseq(), Some(1));
}

#[test]
fn test_decode_split_body() {
    let mut codec = RtspCodec::new();
    let header = b"RTSP/1.0 200 OK\r\nContent-Length: 5\r\n\r\n";
    let body_part1 = b"he";
    let body_part2 = b"llo";

    codec.feed(header).unwrap();
    assert!(codec.decode().unwrap().is_none());

    codec.feed(body_part1).unwrap();
    assert!(codec.decode().unwrap().is_none());

    codec.feed(body_part2).unwrap();
    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.body, b"hello");
}

#[test]
fn test_header_case_insensitivity() {
    let mut codec = RtspCodec::new();
    let data = b"RTSP/1.0 200 OK\r\nCONTENT-LENGTH: 0\r\ncseq: 99\r\n\r\n";

    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();

    assert_eq!(response.cseq(), Some(99));
    assert_eq!(response.headers.content_length(), Some(0));
}

#[test]
fn test_decode_incomplete_header() {
    let mut codec = RtspCodec::new();
    codec.feed(b"RTSP/1.0 200 OK\r\nContent-Len").unwrap();
    assert!(codec.decode().unwrap().is_none());
}

#[test]
fn test_decode_reset() {
    let mut codec = RtspCodec::new();
    codec.feed(b"RTSP/1.0 200 OK").unwrap(); // Incomplete
    codec.reset();
    assert_eq!(codec.buffered_len(), 0);

    // Should be able to decode fresh packet
    codec.feed(b"RTSP/1.0 200 OK\r\n\r\n").unwrap();
    assert!(codec.decode().unwrap().is_some());
}

#[test]
fn test_decode_split_status_line() {
    let mut codec = RtspCodec::new();
    codec.feed(b"RTSP/1.").unwrap();
    assert!(codec.decode().unwrap().is_none());
    codec.feed(b"0 200 OK\r\nCSeq: 1\r\n\r\n").unwrap();
    let response = codec.decode().unwrap().unwrap();
    assert_eq!(response.status, StatusCode::OK);
}

#[test]
fn test_decode_lenient_whitespace() {
    let mut codec = RtspCodec::new();
    // Extra spaces
    let data = b"RTSP/1.0   200   OK\r\nCSeq:   1\r\nContent-Length:   0\r\n\r\n";
    codec.feed(data).unwrap();
    let response = codec.decode().unwrap().unwrap();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.cseq(), Some(1));
}

#[test]
fn test_random_garbage_robustness() {
    use rand::RngCore;
    let mut codec = RtspCodec::new();
    let mut rng = rand::thread_rng();
    let mut data = [0u8; 100];

    for _ in 0..100 {
        rng.fill_bytes(&mut data);
        let _ = codec.feed(&data);
        let _ = codec.decode(); // Should not panic
        codec.reset();
    }
}

#[test]
fn test_decode_malformed_header_no_colon() {
    let mut codec = RtspCodec::new();
    codec
        .feed(b"RTSP/1.0 200 OK\r\nInvalidHeader\r\n\r\n")
        .unwrap();
    let result = codec.decode();
    assert!(matches!(result, Err(RtspCodecError::InvalidHeader(_))));
}

#[test]
fn test_decode_empty_header_name() {
    let mut codec = RtspCodec::new();
    codec
        .feed(b"RTSP/1.0 200 OK\r\n: EmptyName\r\n\r\n")
        .unwrap();
    // Logic: name = line[..0].trim() -> "", value = line[1..].trim() -> "EmptyName"
    let response = codec.decode().unwrap().unwrap();
    assert_eq!(response.headers.get(""), Some("EmptyName"));
}

#[test]
fn test_max_size_exact_limit() {
    let mut codec = RtspCodec::new().with_max_size(10);

    // Feed 10 bytes: should succeed
    let result = codec.feed(&[0u8; 10]);
    assert!(result.is_ok());
    assert_eq!(codec.buffered_len(), 10);

    // Reset
    codec.reset();

    // Feed 11 bytes: should fail
    let result = codec.feed(&[0u8; 11]);
    assert!(matches!(
        result,
        Err(RtspCodecError::ResponseTooLarge { .. })
    ));
}
