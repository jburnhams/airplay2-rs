//! Integration tests for RTSP server codec
//!
//! These tests simulate complete RTSP conversations between
//! a mock sender and our server codec.

use airplay2::protocol::rtsp::Method;
use airplay2::protocol::rtsp::server_codec::{RtspServerCodec, encode_response};
use airplay2::receiver::rtsp_handler::handle_request;
use airplay2::receiver::session::{ReceiverSession, SessionState};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Simulate a complete RAOP session negotiation
#[test]
fn test_complete_session_negotiation() {
    let mut codec = RtspServerCodec::new();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);
    let mut session = ReceiverSession::new(addr);

    // Step 1: OPTIONS
    codec.feed(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n");
    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    let response_str = String::from_utf8(encode_response(&result.response)).unwrap();
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("Public:"));
    assert!(response_str.contains("ANNOUNCE"));

    // Step 2: ANNOUNCE with SDP
    let sdp = "v=0\r\n\
               o=iTunes 1234 0 IN IP4 192.168.1.100\r\n\
               s=iTunes\r\n\
               c=IN IP4 192.168.1.1\r\n\
               t=0 0\r\n\
               m=audio 0 RTP/AVP 96\r\n\
               a=rtpmap:96 AppleLossless\r\n\
               a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n";

    let announce = format!(
        "ANNOUNCE rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
         CSeq: 2\r\n\
         Content-Type: application/sdp\r\n\
         Content-Length: {}\r\n\
         \r\n{}",
        sdp.len(),
        sdp
    );

    codec.clear();
    codec.feed(announce.as_bytes());

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    assert!(
        String::from_utf8(encode_response(&result.response))
            .unwrap()
            .contains("200 OK")
    );
    assert_eq!(result.new_state, Some(SessionState::Announced));

    // Step 3: SETUP
    session.set_state(SessionState::Announced).unwrap();
    codec.clear();
    codec.feed(
        b"SETUP rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 3\r\n\
          Transport: RTP/AVP/UDP;unicast;mode=record;control_port=6001;timing_port=6002\r\n\
          \r\n",
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    let response_str = String::from_utf8(encode_response(&result.response)).unwrap();
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("Session:"));
    assert!(response_str.contains("Transport:"));
    assert_eq!(result.new_state, Some(SessionState::Setup));
    assert!(result.allocated_ports.is_some());

    // Step 4: RECORD
    session.set_state(SessionState::Setup).unwrap();
    codec.clear();
    codec.feed(
        b"RECORD rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 4\r\n\
          Range: npt=0-\r\n\
          RTP-Info: seq=1;rtptime=0\r\n\
          \r\n",
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    let response_str = String::from_utf8(encode_response(&result.response)).unwrap();
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("Audio-Latency:"));
    assert!(result.start_streaming);

    // Step 5: TEARDOWN
    session.set_state(SessionState::Streaming).unwrap();
    codec.clear();
    codec.feed(
        b"TEARDOWN rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 5\r\n\
          \r\n",
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    assert!(
        String::from_utf8(encode_response(&result.response))
            .unwrap()
            .contains("200 OK")
    );
    assert!(result.stop_streaming);
}

/// Test volume control via SET_PARAMETER
#[test]
fn test_volume_control() {
    let mut codec = RtspServerCodec::new();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);
    let session = ReceiverSession::new(addr);

    let volume_cmd = "SET_PARAMETER rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
                      CSeq: 10\r\n\
                      Content-Type: text/parameters\r\n\
                      Content-Length: 20\r\n\
                      \r\n\
                      volume: -15.000000\r\n";

    codec.feed(volume_cmd.as_bytes());
    let request = codec.decode().unwrap().unwrap();

    assert_eq!(request.method, Method::SetParameter);
    let result = handle_request(&request, &session);

    assert!(
        String::from_utf8(encode_response(&result.response))
            .unwrap()
            .contains("200 OK")
    );
}

/// Test keep-alive via empty GET_PARAMETER
#[test]
fn test_keepalive() {
    let mut codec = RtspServerCodec::new();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);
    let session = ReceiverSession::new(addr);

    codec.feed(
        b"GET_PARAMETER rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 20\r\n\
          \r\n",
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    assert!(
        String::from_utf8(encode_response(&result.response))
            .unwrap()
            .contains("200 OK")
    );
}

/// Test FLUSH handling
#[test]
fn test_flush() {
    let mut codec = RtspServerCodec::new();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);
    let session = ReceiverSession::new(addr);

    codec.feed(
        b"FLUSH rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 30\r\n\
          RTP-Info: rtptime=12345\r\n\
          \r\n",
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    assert!(
        String::from_utf8(encode_response(&result.response))
            .unwrap()
            .contains("200 OK")
    );
}

/// Test error handling for invalid state transitions
#[test]
fn test_invalid_state_transition() {
    let mut codec = RtspServerCodec::new();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);
    let session = ReceiverSession::new(addr);
    // Session is in initial state, not Setup

    codec.feed(
        b"RECORD rtsp://192.168.1.1/1234 RTSP/1.0\r\n\
          CSeq: 1\r\n\
          \r\n",
    );

    let request = codec.decode().unwrap().unwrap();
    let result = handle_request(&request, &session);

    // Should get 455 Method Not Valid in This State
    assert!(
        String::from_utf8(encode_response(&result.response))
            .unwrap()
            .contains("455")
    );
}
