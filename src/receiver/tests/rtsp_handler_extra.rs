use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::protocol::rtsp::{Headers, Method, RtspRequest, StatusCode};
use crate::receiver::rtsp_handler::*;
use crate::receiver::session::{ReceiverSession, SessionState};
use crate::receiver::set_parameter_handler::ParameterUpdate;

fn test_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)
}

fn create_request(method: Method) -> RtspRequest {
    RtspRequest {
        method,
        uri: "rtsp://localhost/stream".to_string(),
        headers: Headers::new(),
        body: Vec::new(),
    }
}

#[test]
fn test_post_not_implemented() {
    let session = ReceiverSession::new(test_addr());
    let request = create_request(Method::Post);
    let result = handle_request(&request, &session, None);
    assert_eq!(result.response.status, StatusCode::NOT_IMPLEMENTED);
}

#[test]
fn test_setup_missing_transport() {
    let mut session = ReceiverSession::new(test_addr());
    session.set_state(SessionState::Announced).unwrap();

    let request = create_request(Method::Setup);
    // No Transport header
    let result = handle_request(&request, &session, None);
    assert_eq!(result.response.status, StatusCode::BAD_REQUEST);
}

#[test]
fn test_setup_invalid_transport_format() {
    let mut session = ReceiverSession::new(test_addr());
    session.set_state(SessionState::Announced).unwrap();

    let mut request = create_request(Method::Setup);
    request
        .headers
        .insert("Transport".to_string(), "Invalid;Format;NoRTP".to_string());

    let result = handle_request(&request, &session, None);
    assert_eq!(result.response.status, StatusCode::BAD_REQUEST);
}

#[test]
fn test_set_parameter_progress() {
    let session = ReceiverSession::new(test_addr());
    let mut request = create_request(Method::SetParameter);
    request
        .headers
        .insert("Content-Type".to_string(), "text/parameters".to_string());
    request.body = b"progress: 100/200/500\r\n".to_vec();

    let result = handle_request(&request, &session, None);
    assert_eq!(result.response.status, StatusCode::OK);

    let progress_update = result
        .parameter_updates
        .iter()
        .find(|u| matches!(u, ParameterUpdate::Progress(_)));
    assert!(progress_update.is_some());
}

#[test]
fn test_set_parameter_metadata() {
    let session = ReceiverSession::new(test_addr());
    let mut request = create_request(Method::SetParameter);
    request.headers.insert(
        "Content-Type".to_string(),
        "application/x-dmap-tagged".to_string(),
    );
    // Minimal valid DMAP (MLIT container)
    // 'mlit' (4) + length (4)
    let dmap_data = vec![
        0x6D, 0x6C, 0x69, 0x74, // mlit
        0x00, 0x00, 0x00, 0x00, // length 0
    ];
    request.body = dmap_data;

    let result = handle_request(&request, &session, None);
    assert_eq!(result.response.status, StatusCode::OK);

    let metadata_update = result
        .parameter_updates
        .iter()
        .find(|u| matches!(u, ParameterUpdate::Metadata(_)));
    assert!(metadata_update.is_some());
}

#[test]
fn test_get_parameter_multiple() {
    let mut session = ReceiverSession::new(test_addr());
    session.set_volume(-10.0);

    let mut request = create_request(Method::GetParameter);
    // Request volume and unknown param
    request.body = b"volume\r\nunknown_param".to_vec();

    let result = handle_request(&request, &session, None);
    assert_eq!(result.response.status, StatusCode::OK);

    let body = String::from_utf8(result.response.body).unwrap();
    // Should contain volume but ignore unknown
    assert!(body.contains("volume: -10.000000"));
    assert!(!body.contains("unknown_param"));
}

#[test]
fn test_unsupported_method() {
    // RTSP standard defines Redirect but AirPlay doesn't implement it
    // Method enum doesn't have Redirect, so we can't create it directly via create_request
    // But we can check Method::Get behavior which is basic
    let session = ReceiverSession::new(test_addr());
    let request = create_request(Method::Get);
    // GET might be allowed or not depending on state, but handle_request might treat it as
    // default/unknown Let's check handle_request impl.
    // match request.method { ... _ => handle_unknown(cseq) }
    // GET is explicitly handled? No, it falls to handle_unknown if not in the match arms.
    // Looking at `handle_request`:
    // match request.method {
    //     Options => ..., Announce => ..., Setup => ..., Record => ..., Pause => ..., Flush => ...,
    // Teardown => ...,     GetParameter => ..., SetParameter => ..., Post => ...,
    //     _ => handle_unknown(cseq),
    // }
    // GET is NOT in the list, so it should be Method Not Allowed
    let result = handle_request(&request, &session, None);
    assert_eq!(result.response.status, StatusCode::METHOD_NOT_ALLOWED);
}
