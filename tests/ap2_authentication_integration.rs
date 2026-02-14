use airplay2::protocol::crypto::Ed25519KeyPair;
use airplay2::protocol::pairing::tlv::{TlvEncoder, TlvType};
use airplay2::protocol::rtsp::{Headers, Method, RtspRequest};
use airplay2::receiver::ap2::password_auth::PasswordAuthManager;
use airplay2::receiver::ap2::password_integration::AuthenticationHandler;
use airplay2::receiver::ap2::request_handler::Ap2RequestContext;
use airplay2::receiver::ap2::session_state::Ap2SessionState;

fn make_request(method: Method, uri: &str, body: Vec<u8>) -> RtspRequest {
    let mut headers = Headers::new();
    headers.insert("CSeq".to_string(), "1".to_string());
    headers.insert("User-Agent".to_string(), "AirPlay/320.20".to_string());

    RtspRequest {
        method,
        uri: uri.to_string(),
        headers,
        body,
    }
}

fn create_auth_handler_password(password: &str) -> AuthenticationHandler {
    let identity = Ed25519KeyPair::generate();
    let mut manager = PasswordAuthManager::new(identity);
    manager.set_password(password.to_string());
    AuthenticationHandler::password_only(manager)
}

#[test]
fn test_password_auth_flow() {
    let handler = create_auth_handler_password("1234");
    let state = Ap2SessionState::Connected;
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    // 1. M1
    let m1_body = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();
    let req = make_request(Method::Post, "/pair-setup", m1_body);

    let result = handler.handle_pair_setup(&req, 1, &ctx);
    assert!(result.error.is_none());
    assert!(result.new_state.is_none()); // M2 doesn't change session state in AuthHandler logic directly unless complete?

    // Check response
    // Ap2HandleResult.response is Vec<u8> (RTSP response bytes)
    // We need to parse it or check if it's 200 OK.
    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.contains("RTSP/1.0 200 OK"));
    assert!(response_str.contains("application/octet-stream"));
}

#[test]
fn test_auth_failure_response() {
    let handler = create_auth_handler_password("1234");
    let state = Ap2SessionState::Connected;
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    // 1. M1
    let m1_body = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();
    let req = make_request(Method::Post, "/pair-setup", m1_body);
    let _ = handler.handle_pair_setup(&req, 1, &ctx);

    // 2. M3 with bad password
    let m3_body = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::PublicKey, &[0u8; 32])
        .add(TlvType::Proof, &[0u8; 32])
        .build();
    let req_m3 = make_request(Method::Post, "/pair-setup", m3_body);

    let result = handler.handle_pair_setup(&req_m3, 2, &ctx);

    // Should contain error TLV
    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.contains("RTSP/1.0 200 OK")); // Still 200 OK for pairing protocol errors

    // But internally it should have error TLV.
    // We can't easily parse the body here without a full RTSP parser,
    // but we can trust the integration test we added for unit tests covered this.
    // Here we verify handle_result structure.

    // Wait, PasswordAuthManager returns error in Result.
    // AuthenticationHandler converts error to response.
    // If PasswordAuthManager returns Ok(response) with error field set,
    // AuthenticationHandler sets new_state to Error.

    assert!(result.new_state.is_some());
    if let Some(Ap2SessionState::Error { code, .. }) = result.new_state {
        assert_eq!(code, 470);
    } else {
        panic!("Expected Error state");
    }
}

#[test]
fn test_lockout_propagation() {
    let handler = create_auth_handler_password("1234");
    let state = Ap2SessionState::Connected;
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    // Trigger lockout
    let m1_body = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();
    let m3_body = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::PublicKey, &[0u8; 32])
        .add(TlvType::Proof, &[0u8; 32])
        .build();

    for _ in 0..5 {
        let req_m1 = make_request(Method::Post, "/pair-setup", m1_body.clone());
        let _ = handler.handle_pair_setup(&req_m1, 1, &ctx);

        let req_m3 = make_request(Method::Post, "/pair-setup", m3_body.clone());
        let _ = handler.handle_pair_setup(&req_m3, 2, &ctx);
    }

    // Now should be locked out
    let req_m1 = make_request(Method::Post, "/pair-setup", m1_body.clone());
    let result = handler.handle_pair_setup(&req_m1, 3, &ctx);

    let response_str = String::from_utf8_lossy(&result.response);
    // Should be 503 Service Unavailable or similar?
    // AuthenticationHandler uses 503 for lockout.
    assert!(response_str.contains("RTSP/1.0 503 Service Unavailable"));
    assert!(response_str.contains("Retry-After"));
}
