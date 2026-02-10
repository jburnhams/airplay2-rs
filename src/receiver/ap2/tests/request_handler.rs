use crate::protocol::rtsp::{Headers, Method, RtspRequest};
use crate::receiver::ap2::request_handler::{
    Ap2HandleResult, Ap2Handlers, Ap2RequestContext, handle_ap2_request,
};
use crate::receiver::ap2::response_builder::Ap2ResponseBuilder;
use crate::receiver::ap2::session_state::Ap2SessionState;

fn make_request(method: Method, uri: &str) -> RtspRequest {
    let mut headers = Headers::new();
    headers.insert("CSeq".to_string(), "1".to_string());

    RtspRequest {
        method,
        uri: uri.to_string(),
        headers,
        body: vec![],
    }
}

fn make_context() -> Ap2RequestContext<'static> {
    Ap2RequestContext {
        state: &Ap2SessionState::Connected,
        session_id: None,
        encrypted: false,
        decrypt: None,
    }
}

#[test]
fn test_options_always_allowed() {
    let request = make_request(Method::Options, "*");
    let context = make_context();
    let handlers = Ap2Handlers::default();

    let result = handle_ap2_request(&request, &context, &handlers);

    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("Public:"));
}

#[test]
fn test_unauthenticated_command_rejected() {
    let request = make_request(Method::Post, "/command");
    let context = make_context();
    let handlers = Ap2Handlers::default();

    let result = handle_ap2_request(&request, &context, &handlers);

    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.contains("470")); // Auth required
}

#[test]
fn test_pair_setup_allowed_unauthenticated() {
    let request = make_request(Method::Post, "/pair-setup");

    // Use a handler that returns OK
    let mut handlers = Ap2Handlers::default();
    handlers.pair_setup = |_, cseq, _| Ap2HandleResult {
        response: Ap2ResponseBuilder::ok().cseq(cseq).encode(),
        new_state: None,
        event: None,
        error: None,
    };

    let context = make_context();
    let result = handle_ap2_request(&request, &context, &handlers);

    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.contains("200 OK"));
}

#[test]
fn test_setup_requires_paired_state() {
    let request = make_request(Method::Setup, "rtsp://192.168.1.1/12345");

    let context = Ap2RequestContext {
        state: &Ap2SessionState::Connected,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let handlers = Ap2Handlers::default();
    let result = handle_ap2_request(&request, &context, &handlers);

    let response_str = String::from_utf8_lossy(&result.response);
    // Should be rejected - not in paired state
    // METHOD_NOT_VALID is 455
    assert!(response_str.contains("455") || response_str.contains("Not Valid"));
}
