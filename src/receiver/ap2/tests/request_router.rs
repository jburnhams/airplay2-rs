use crate::protocol::rtsp::{Headers, Method, RtspRequest};
use crate::receiver::ap2::request_router::{Ap2Endpoint, Ap2RequestType, RtspMethod};

#[test]
fn test_classify_rtsp_methods() {
    let request = RtspRequest {
        method: Method::Setup,
        uri: "rtsp://192.168.1.1/12345".to_string(),
        headers: Headers::new(),
        body: vec![],
    };

    assert_eq!(
        Ap2RequestType::classify(&request),
        Ap2RequestType::Rtsp(RtspMethod::Setup)
    );
}

#[test]
fn test_classify_post_endpoints() {
    let request = RtspRequest {
        method: Method::Post,
        uri: "/pair-setup".to_string(),
        headers: Headers::new(),
        body: vec![],
    };

    assert_eq!(
        Ap2RequestType::classify(&request),
        Ap2RequestType::Endpoint(Ap2Endpoint::PairSetup)
    );
}

#[test]
fn test_classify_get_info() {
    let request = RtspRequest {
        method: Method::Get,
        uri: "rtsp://192.168.1.100:7000/info".to_string(),
        headers: Headers::new(),
        body: vec![],
    };

    assert_eq!(
        Ap2RequestType::classify(&request),
        Ap2RequestType::Endpoint(Ap2Endpoint::Info)
    );
}

#[test]
fn test_auth_requirements() {
    assert!(!Ap2Endpoint::PairSetup.requires_auth());
    assert!(!Ap2Endpoint::PairVerify.requires_auth());
    assert!(Ap2Endpoint::Command.requires_auth());
    assert!(Ap2Endpoint::Feedback.requires_auth());
}

#[test]
fn test_expects_bplist() {
    assert!(!Ap2Endpoint::Info.expects_bplist());
    assert!(Ap2Endpoint::PairSetup.expects_bplist());
    assert!(Ap2Endpoint::Command.expects_bplist());
}

#[test]
fn test_unknown_endpoint() {
    let request = RtspRequest {
        method: Method::Post,
        uri: "/unknown".to_string(),
        headers: Headers::new(),
        body: vec![],
    };

    match Ap2RequestType::classify(&request) {
        Ap2RequestType::Endpoint(Ap2Endpoint::Unknown(path)) => {
            assert_eq!(path, "/unknown");
        }
        _ => panic!("Expected unknown endpoint"),
    }
}
