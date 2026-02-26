use crate::protocol::crypto::Ed25519KeyPair;
use crate::protocol::rtsp::{Headers, Method, RtspRequest};
use crate::receiver::ap2::pairing_handlers::PairingHandler;
use crate::receiver::ap2::pairing_server::PairingServer;

#[test]
fn test_handle_pair_setup_empty_body() {
    let identity = Ed25519KeyPair::generate();
    let server = PairingServer::new(identity);
    let handler = PairingHandler::new(server);

    let request = RtspRequest {
        method: Method::Post,
        uri: "/pair-setup".to_string(),
        headers: Headers::default(),
        body: vec![],
    };

    let result = handler.handle_pair_setup(&request, 1);

    // Should be Bad Request or Error
    assert!(result.error.is_some());
    assert_eq!(result.error.unwrap(), "Empty pair-setup body");
}

#[test]
fn test_handle_pair_setup_success() {
    let identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");
    let handler = PairingHandler::new(server);

    let body = vec![
        0x06, 0x01, 0x01, // kTLVType_State, 1
        0x00, 0x01, 0x00, // kTLVType_Method, 0
    ];

    let request = RtspRequest {
        method: Method::Post,
        uri: "/pair-setup".to_string(),
        headers: Headers::default(),
        body,
    };

    let result = handler.handle_pair_setup(&request, 1);

    assert!(result.error.is_none());
    // M2 response
}

#[test]
fn test_handle_pair_verify_empty_body() {
    let identity = Ed25519KeyPair::generate();
    let server = PairingServer::new(identity);
    let handler = PairingHandler::new(server);

    let request = RtspRequest {
        method: Method::Post,
        uri: "/pair-verify".to_string(),
        headers: Headers::default(),
        body: vec![],
    };

    let result = handler.handle_pair_verify(&request, 1);

    assert!(result.error.is_some());
    assert_eq!(result.error.unwrap(), "Empty pair-verify body");
}
