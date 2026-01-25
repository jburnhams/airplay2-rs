use super::*;
use crate::protocol::rtsp::headers::names;

#[test]
fn test_spec_compliance_options_request() {
    let mut session = RtspSession::new("127.0.0.1", 5000);
    let request = session.options_request();

    // Spec Requirement: Must contain CSeq
    assert!(
        request.headers.cseq().is_some(),
        "OPTIONS request must contain CSeq"
    );

    // Spec Requirement: Must contain User-Agent
    assert!(
        request.headers.get(names::USER_AGENT).is_some(),
        "OPTIONS request must contain User-Agent"
    );

    // RAOP Mandatory Headers
    assert!(
        request.headers.get(names::X_APPLE_DEVICE_ID).is_some(),
        "Missing X-Apple-Device-ID"
    );
    assert!(
        request.headers.get(names::X_APPLE_SESSION_ID).is_some(),
        "Missing X-Apple-Session-ID"
    );
    assert!(
        request.headers.get(names::DACP_ID).is_some(),
        "Missing DACP-ID"
    );
    assert!(
        request.headers.get(names::ACTIVE_REMOTE).is_some(),
        "Missing Active-Remote"
    );
}

#[test]
fn test_spec_compliance_announce_request() {
    let mut session = RtspSession::new("127.0.0.1", 5000);
    let sdp = "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=AirPlay\r\nt=0 0\r\n";
    let request = session.announce_request(sdp);

    assert_eq!(request.method, Method::Announce);
    assert_eq!(request.headers.content_type(), Some("application/sdp"));
    assert_eq!(request.headers.content_length(), Some(sdp.len()));

    // Ensure CSeq incremented
    // Note: Depends on previous calls. New session starts at 0, next_cseq increments.
    // options_request() was not called on this session, so first request is 1.
    assert_eq!(request.headers.cseq(), Some(1));
}

#[test]
fn test_spec_compliance_setup_request() {
    let mut session = RtspSession::new("127.0.0.1", 5000);
    let request = session.setup_stream_request("RTP/AVP/UDP;unicast");

    assert_eq!(request.method, Method::Setup);
    assert_eq!(
        request.headers.get(names::TRANSPORT),
        Some("RTP/AVP/UDP;unicast")
    );
}

#[test]
fn test_spec_compliance_user_agent_format() {
    let session = RtspSession::new("127.0.0.1", 5000);
    let ua = session.user_agent();

    // AirPlay User-Agent usually follows "AirPlay/<version>"
    assert!(
        ua.starts_with("AirPlay/"),
        "User-Agent must start with AirPlay/"
    );
}
