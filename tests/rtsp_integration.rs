use airplay2::protocol::rtsp::{Method, RtspCodec, RtspSession, SessionState};

#[test]
fn test_full_session_flow() {
    let mut session = RtspSession::new("192.168.1.10", 7000);
    let mut codec = RtspCodec::new();

    // 1. OPTIONS
    let options = session.options_request();
    assert!(!options.encode().is_empty());

    // Simulate response
    codec
        .feed(b"RTSP/1.0 200 OK\r\nCSeq: 1\r\nPublic: SETUP, RECORD\r\n\r\n")
        .unwrap();
    let response = codec.decode().unwrap().unwrap();
    session
        .process_response(Method::Options, &response)
        .unwrap();

    assert_eq!(session.state(), SessionState::Ready);

    // 2. SETUP
    let setup = session.setup_request("RTP/AVP/UDP;unicast;mode=record");
    assert!(!setup.encode().is_empty());

    // Simulate response with session ID
    codec.reset();
    codec
        .feed(b"RTSP/1.0 200 OK\r\nCSeq: 2\r\nSession: DEADBEEF;timeout=30\r\n\r\n")
        .unwrap();
    let response = codec.decode().unwrap().unwrap();
    session.process_response(Method::Setup, &response).unwrap();

    assert_eq!(session.state(), SessionState::Setup);
    assert_eq!(session.session_id(), Some("DEADBEEF"));

    // 3. RECORD
    let record = session.record_request();
    assert!(!record.encode().is_empty());
    // RECORD includes session ID
    let record_str = String::from_utf8_lossy(&record.encode()).to_string();
    assert!(record_str.contains("Session: DEADBEEF"));

    codec.reset();
    codec
        .feed(b"RTSP/1.0 200 OK\r\nCSeq: 3\r\n\r\n")
        .unwrap();
    let response = codec.decode().unwrap().unwrap();
    session.process_response(Method::Record, &response).unwrap();

    assert_eq!(session.state(), SessionState::Playing);

    // 4. SET_PARAMETER (Volume)
    let body = b"volume: -10.0".to_vec();
    let set_param = session.set_parameter_request("text/parameters", body);
    assert!(!set_param.encode().is_empty());

    codec.reset();
    codec
        .feed(b"RTSP/1.0 200 OK\r\nCSeq: 4\r\n\r\n")
        .unwrap();
    let response = codec.decode().unwrap().unwrap();
    session
        .process_response(Method::SetParameter, &response)
        .unwrap();

    assert_eq!(session.state(), SessionState::Playing);

    // 5. TEARDOWN
    let teardown = session.teardown_request();
    assert!(!teardown.encode().is_empty());

    codec.reset();
    codec
        .feed(b"RTSP/1.0 200 OK\r\nCSeq: 5\r\n\r\n")
        .unwrap();
    let response = codec.decode().unwrap().unwrap();
    session
        .process_response(Method::Teardown, &response)
        .unwrap();

    assert_eq!(session.state(), SessionState::Terminated);
}
