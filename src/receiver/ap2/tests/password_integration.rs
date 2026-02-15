use crate::protocol::crypto::Ed25519KeyPair;
use crate::protocol::rtsp::{Method, RtspCodec, RtspRequest, StatusCode};
use crate::receiver::ap2::pairing_handlers::PairingHandler;
use crate::receiver::ap2::pairing_server::PairingServer;
use crate::receiver::ap2::password_auth::PasswordAuthManager;
use crate::receiver::ap2::password_integration::AuthenticationHandler;
use crate::receiver::ap2::request_handler::Ap2RequestContext;
use crate::receiver::ap2::session_state::Ap2SessionState;

fn parse_response(bytes: &[u8]) -> crate::protocol::rtsp::RtspResponse {
    let mut codec = RtspCodec::new();
    codec.feed(bytes).expect("Feed bytes");
    codec.decode().expect("Decode").expect("Complete response")
}

#[test]
fn test_password_only_mode() {
    let identity = Ed25519KeyPair::generate();
    let mut password_manager = PasswordAuthManager::new(identity);
    password_manager.set_password("1234".to_string());

    let handler = AuthenticationHandler::password_only(password_manager);
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    // Create a dummy pair-setup request (M1)
    // Tlv: State=1, Method=0
    let body = vec![
        0x06, 0x01, 0x01, // kTLVType_State, 1
        0x00, 0x01, 0x00, // kTLVType_Method, 0
    ];

    let request = RtspRequest {
        method: Method::Post,
        uri: "/pair-setup".to_string(),
        headers: Default::default(),
        body,
    };

    let result = handler.handle_pair_setup(&request, 1, &context);

    // Should return 200 OK (M2)
    let response = parse_response(&result.response);
    assert_eq!(response.status, StatusCode::OK);
    assert!(!response.body.is_empty());
}

#[test]
fn test_password_only_mode_disabled() {
    let identity = Ed25519KeyPair::generate();
    let mut password_manager = PasswordAuthManager::new(identity);
    password_manager.clear_password(); // Disable

    let handler = AuthenticationHandler::password_only(password_manager);
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let request = RtspRequest {
        method: Method::Post,
        uri: "/pair-setup".to_string(),
        headers: Default::default(),
        body: vec![0x06, 0x01, 0x01, 0x00, 0x01, 0x00],
    };

    let result = handler.handle_pair_setup(&request, 1, &context);

    // Should fail (Not Implemented)
    let response = parse_response(&result.response);
    assert_eq!(response.status, StatusCode::NOT_IMPLEMENTED);
}

#[test]
fn test_homekit_only_mode() {
    let identity = Ed25519KeyPair::generate();
    let server = PairingServer::new(identity);
    let pairing_handler = PairingHandler::new(server);

    let handler = AuthenticationHandler::homekit_only(pairing_handler);
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    // M1
    let body = vec![0x06, 0x01, 0x01, 0x00, 0x01, 0x00];
    let request = RtspRequest {
        method: Method::Post,
        uri: "/pair-setup".to_string(),
        headers: Default::default(),
        body,
    };

    let result = handler.handle_pair_setup(&request, 1, &context);

    let response = parse_response(&result.response);
    assert_eq!(response.status, StatusCode::OK);
    assert!(!response.body.is_empty());
}

#[test]
fn test_both_modes_password_priority() {
    let identity1 = Ed25519KeyPair::generate();
    let mut password_manager = PasswordAuthManager::new(identity1);
    password_manager.set_password("1234".to_string());

    let identity2 = Ed25519KeyPair::generate();
    let server = PairingServer::new(identity2);
    let pairing_handler = PairingHandler::new(server);

    let handler = AuthenticationHandler::both(password_manager, pairing_handler);
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    // M1
    let body = vec![0x06, 0x01, 0x01, 0x00, 0x01, 0x00];
    let request = RtspRequest {
        method: Method::Post,
        uri: "/pair-setup".to_string(),
        headers: Default::default(),
        body,
    };

    let result = handler.handle_pair_setup(&request, 1, &context);

    let response = parse_response(&result.response);
    assert_eq!(response.status, StatusCode::OK);
}

#[test]
fn test_both_modes_fallback() {
    let identity1 = Ed25519KeyPair::generate();
    let mut password_manager = PasswordAuthManager::new(identity1);
    password_manager.clear_password(); // Disabled

    let identity2 = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(identity2);
    server.set_password("5678");
    let pairing_handler = PairingHandler::new(server);

    let handler = AuthenticationHandler::both(password_manager, pairing_handler);
    let state = Ap2SessionState::Connected;
    let context = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    // M1
    let body = vec![0x06, 0x01, 0x01, 0x00, 0x01, 0x00];
    let request = RtspRequest {
        method: Method::Post,
        uri: "/pair-setup".to_string(),
        headers: Default::default(),
        body,
    };

    let result = handler.handle_pair_setup(&request, 1, &context);

    let response = parse_response(&result.response);
    assert_eq!(response.status, StatusCode::OK);
    assert!(!response.body.is_empty());
}
