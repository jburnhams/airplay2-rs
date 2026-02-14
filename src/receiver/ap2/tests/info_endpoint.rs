use crate::protocol::rtsp::{Headers, Method, RtspRequest};
use crate::receiver::ap2::capabilities::DeviceCapabilities;
use crate::receiver::ap2::info_endpoint::InfoEndpoint;
use crate::receiver::ap2::request_handler::Ap2RequestContext;
use crate::receiver::ap2::session_state::Ap2SessionState;

fn make_info_request() -> RtspRequest {
    let mut headers = Headers::new();
    headers.insert("CSeq".to_string(), "1".to_string());

    RtspRequest {
        method: Method::Get,
        uri: "/info".to_string(),
        headers,
        body: vec![],
    }
}

#[test]
fn test_info_response() {
    let caps = DeviceCapabilities::audio_receiver("AA:BB:CC:DD:EE:FF", "Test Speaker", [0u8; 32]);
    let endpoint = InfoEndpoint::new(caps);

    let request = make_info_request();
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let result = endpoint.handle(&request, 1, &context);

    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("application/x-apple-binary-plist"));
}

#[test]
fn test_state_transition() {
    let caps = DeviceCapabilities::audio_receiver("AA:BB:CC:DD:EE:FF", "Test Speaker", [0u8; 32]);
    let endpoint = InfoEndpoint::new(caps);

    let request = make_info_request();
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let result = endpoint.handle(&request, 1, &context);

    assert!(matches!(
        result.new_state,
        Some(Ap2SessionState::InfoExchanged)
    ));
}
