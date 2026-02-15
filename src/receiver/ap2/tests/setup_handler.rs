use crate::protocol::plist::PlistValue;
use crate::protocol::rtsp::{Method, RtspRequest};
use crate::receiver::ap2::body_handler::{encode_bplist_body, parse_bplist_body};
use crate::receiver::ap2::request_handler::{Ap2Event, Ap2RequestContext};
use crate::receiver::ap2::session_state::Ap2SessionState;
use crate::receiver::ap2::setup_handler::{SetupHandler, SetupPhase};
use crate::receiver::ap2::stream::{EncryptionType, TimingProtocol};
use std::collections::HashMap;

fn create_setup_request(body: &[u8]) -> RtspRequest {
    RtspRequest::builder(Method::Setup, "rtsp://localhost/stream")
        .body(body.to_vec())
        .build()
}

fn create_phase1_plist() -> PlistValue {
    let mut dict = HashMap::new();
    dict.insert(
        "timingProtocol".to_string(),
        PlistValue::String("NTP".to_string()),
    );

    let mut streams = Vec::new();

    // Event stream
    let mut event_dict = HashMap::new();
    event_dict.insert("type".to_string(), PlistValue::Integer(130)); // Event
    streams.push(PlistValue::Dictionary(event_dict));

    // Timing stream
    let mut timing_dict = HashMap::new();
    timing_dict.insert("type".to_string(), PlistValue::Integer(150)); // Timing
    streams.push(PlistValue::Dictionary(timing_dict));

    dict.insert("streams".to_string(), PlistValue::Array(streams));

    // Timing peer info
    let mut peer_info = HashMap::new();
    peer_info.insert("ID".to_string(), PlistValue::Integer(12345));
    dict.insert(
        "timingPeerInfo".to_string(),
        PlistValue::Dictionary(peer_info),
    );

    PlistValue::Dictionary(dict)
}

fn create_phase2_plist() -> PlistValue {
    let mut dict = HashMap::new();

    let mut streams = Vec::new();

    // Audio stream
    let mut audio_dict = HashMap::new();
    audio_dict.insert("type".to_string(), PlistValue::Integer(96)); // Audio
    audio_dict.insert("ct".to_string(), PlistValue::Integer(0x1)); // PCM
    streams.push(PlistValue::Dictionary(audio_dict));

    dict.insert("streams".to_string(), PlistValue::Array(streams));
    dict.insert("et".to_string(), PlistValue::Integer(4)); // ChaCha20

    PlistValue::Dictionary(dict)
}

fn parse_response(response: &[u8]) -> (String, Vec<u8>) {
    let mut headers_end = 0;
    for i in 0..response.len() - 3 {
        if &response[i..i + 4] == b"\r\n\r\n" {
            headers_end = i;
            break;
        }
    }

    let headers = String::from_utf8_lossy(&response[0..headers_end]).to_string();
    let body = response[headers_end + 4..].to_vec();
    (headers, body)
}

#[test]
fn test_setup_phase1() {
    let handler = SetupHandler::new(50000, 50100, 22050);
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let body = encode_bplist_body(&create_phase1_plist()).unwrap();
    let request = create_setup_request(&body);

    let result = handler.handle(&request, 1, &context);

    assert!(result.error.is_none());
    assert!(result.new_state.is_some());
    assert!(matches!(
        result.new_state.unwrap(),
        Ap2SessionState::SetupPhase1
    ));

    if let Some(Ap2Event::SetupPhase1Complete {
        timing_port,
        event_port,
        timing_protocol,
        ..
    }) = result.event
    {
        assert!(timing_port >= 50000);
        assert!(event_port >= 50000);
        assert_ne!(timing_port, event_port);
        assert_eq!(timing_protocol, TimingProtocol::Ntp);
    } else {
        panic!("Wrong event type");
    }

    let phase = handler.current_phase.lock().unwrap();
    assert!(matches!(*phase, SetupPhase::Phase1Complete));
}

#[test]
fn test_setup_phase2() {
    let handler = SetupHandler::new(50000, 50100, 22050);
    // Simulate phase 1 complete
    {
        let mut phase = handler.current_phase.lock().unwrap();
        *phase = SetupPhase::Phase1Complete;
    }

    let state = Ap2SessionState::SetupPhase1;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let body = encode_bplist_body(&create_phase2_plist()).unwrap();
    let request = create_setup_request(&body);

    let result = handler.handle(&request, 1, &context);

    assert!(result.error.is_none());
    assert!(result.new_state.is_some());
    assert!(matches!(
        result.new_state.unwrap(),
        Ap2SessionState::SetupPhase2
    ));

    if let Some(Ap2Event::SetupPhase2Complete {
        audio_data_port,
        audio_control_port,
        encryption_type,
        ..
    }) = result.event
    {
        assert!(audio_data_port >= 50000);
        assert!(audio_control_port >= 50000);
        assert_ne!(audio_data_port, audio_control_port);
        assert_eq!(encryption_type, EncryptionType::ChaCha20Poly1305);
    } else {
        panic!("Wrong event type");
    }

    let phase = handler.current_phase.lock().unwrap();
    assert!(matches!(*phase, SetupPhase::Phase2Complete));
}

#[test]
fn test_port_allocation_exhaustion() {
    let handler = SetupHandler::new(50000, 50002, 22050); // Only 3 ports available (50000, 50001, 50002)
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    // Phase 1 (uses 2 ports)
    let body1 = encode_bplist_body(&create_phase1_plist()).unwrap();
    let req1 = create_setup_request(&body1);
    let res1 = handler.handle(&req1, 1, &context);
    assert!(res1.error.is_none());

    // Phase 2 (needs 2 more ports, but only 1 left)
    let body2 = encode_bplist_body(&create_phase2_plist()).unwrap();
    let req2 = create_setup_request(&body2);
    let res2 = handler.handle(&req2, 2, &context);

    assert!(res2.error.is_some()); // Should fail

    // Response should indicate error
    let (headers, _) = parse_response(&res2.response);
    let status_line = headers.lines().next().unwrap();
    assert!(status_line.contains("453")); // Not Enough Bandwidth
}

#[test]
fn test_cleanup() {
    let handler = SetupHandler::new(50000, 50100, 22050);
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    // Phase 1
    let body = encode_bplist_body(&create_phase1_plist()).unwrap();
    let req = create_setup_request(&body);
    handler.handle(&req, 1, &context);

    assert!(matches!(
        *handler.current_phase.lock().unwrap(),
        SetupPhase::Phase1Complete
    ));

    // Cleanup
    handler.cleanup();

    assert!(matches!(
        *handler.current_phase.lock().unwrap(),
        SetupPhase::None
    ));
}

#[test]
fn test_response_contains_ports() {
    let handler = SetupHandler::new(50000, 50100, 22050);
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let body = encode_bplist_body(&create_phase1_plist()).unwrap();
    let request = create_setup_request(&body);

    let result = handler.handle(&request, 1, &context);

    // Check response body
    let (_, body) = parse_response(&result.response);

    let plist = parse_bplist_body(&body).expect("Failed to parse response plist");

    if let PlistValue::Dictionary(dict) = plist {
        assert!(dict.contains_key("eventPort"));
        assert!(dict.contains_key("timingPort"));
        assert!(dict.contains_key("streams"));
    } else {
        panic!("Response is not a dictionary");
    }
}
