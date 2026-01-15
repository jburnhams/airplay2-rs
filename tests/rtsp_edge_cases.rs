use airplay2::protocol::rtsp::{Method, RtspCodec, RtspSession, SessionState};

#[test]
fn test_recovery_from_bad_response() {
    let mut session = RtspSession::new("192.168.1.10", 7000);
    let mut codec = RtspCodec::new();

    // 1. Good OPTIONS
    let _ = session.options_request();
    codec.feed(b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n").unwrap();
    let response = codec.decode().unwrap().unwrap();
    session
        .process_response(Method::Options, &response)
        .unwrap();
    assert_eq!(session.state(), SessionState::Ready);

    // 2. Bad Response (Garbage)
    codec.reset();
    codec.feed(b"NOT RTSP GARBAGE\r\n\r\n").unwrap();
    let result = codec.decode();
    assert!(result.is_err());

    // Session state should remain Ready (we wouldn't call process_response)
    assert_eq!(session.state(), SessionState::Ready);

    // 3. Recover with Good SETUP
    codec.reset();
    let _ = session.setup_request("transport");
    codec
        .feed(b"RTSP/1.0 200 OK\r\nCSeq: 2\r\nSession: ID\r\n\r\n")
        .unwrap();
    let response = codec.decode().unwrap().unwrap();
    session.process_response(Method::Setup, &response).unwrap();

    assert_eq!(session.state(), SessionState::Setup);
}

#[test]
fn test_cseq_continuity() {
    let mut session = RtspSession::new("192.168.1.10", 7000);

    let r1 = session.options_request();
    assert_eq!(r1.headers.cseq(), Some(1));

    let r2 = session.setup_request("t");
    assert_eq!(r2.headers.cseq(), Some(2));

    let r3 = session.record_request();
    assert_eq!(r3.headers.cseq(), Some(3));
}
