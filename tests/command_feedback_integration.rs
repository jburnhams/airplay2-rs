use std::collections::HashMap;

use airplay2::protocol::plist::PlistValue;
use airplay2::protocol::rtsp::codec::RtspCodec;
use airplay2::protocol::rtsp::{Method, RtspRequest, RtspResponse, StatusCode};
use airplay2::receiver::ap2::body_handler::{PlistExt, encode_bplist_body, parse_bplist_body};
use airplay2::receiver::ap2::request_handler::{
    Ap2Handlers, Ap2RequestContext, handle_ap2_request,
};
use airplay2::receiver::ap2::session_state::Ap2SessionState;

fn parse_response(bytes: &[u8]) -> RtspResponse {
    let mut codec = RtspCodec::new();
    let _ = codec.feed(bytes);
    codec.decode().unwrap().unwrap()
}

#[test]
fn test_command_endpoint_success() {
    let handlers = Ap2Handlers::default();
    let state = Ap2SessionState::Streaming;

    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("play".to_string()));
    let command_plist = PlistValue::Dictionary(dict);
    let body = encode_bplist_body(&command_plist).unwrap();

    let request = RtspRequest::builder(Method::Post, "/command")
        .cseq(1)
        .header("Content-Type", "application/x-apple-binary-plist")
        .body(body)
        .build();

    let context = Ap2RequestContext {
        state: &state,
        session_id: Some("test-session"),
        encrypted: false,
        decrypt: None,
    };

    let result = handle_ap2_request(&request, &context, &handlers);

    let response = parse_response(&result.response);
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.headers.get("Content-Type"),
        Some("application/x-apple-binary-plist")
    );

    let resp_plist = parse_bplist_body(&response.body).unwrap();
    assert_eq!(resp_plist.get_int("status"), Some(0));
}

#[test]
fn test_feedback_endpoint_success() {
    let handlers = Ap2Handlers::default();
    let state = Ap2SessionState::Streaming;

    let mut dict = HashMap::new();
    dict.insert("timestamp".to_string(), PlistValue::Integer(123456789));
    let feedback_plist = PlistValue::Dictionary(dict);
    let body = encode_bplist_body(&feedback_plist).unwrap();

    let request = RtspRequest::builder(Method::Post, "/feedback")
        .cseq(2)
        .header("Content-Type", "application/x-apple-binary-plist")
        .body(body)
        .build();

    let context = Ap2RequestContext {
        state: &state,
        session_id: Some("test-session"),
        encrypted: false,
        decrypt: None,
    };

    let result = handle_ap2_request(&request, &context, &handlers);

    let response = parse_response(&result.response);
    assert_eq!(response.status, StatusCode::OK);
}
