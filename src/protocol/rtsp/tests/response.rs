use crate::protocol::rtsp::{Headers, RtspResponse, StatusCode};

#[test]
fn test_status_code_classification() {
    assert!(StatusCode(200).is_success());
    assert!(StatusCode(201).is_success());
    assert!(!StatusCode(200).is_client_error());
    assert!(!StatusCode(200).is_server_error());

    assert!(StatusCode(400).is_client_error());
    assert!(StatusCode(404).is_client_error());
    assert!(!StatusCode(404).is_success());

    assert!(StatusCode(500).is_server_error());
    assert!(StatusCode(503).is_server_error());
    assert!(!StatusCode(500).is_success());
}

#[test]
fn test_response_is_plist() {
    let mut headers = Headers::new();
    headers.insert("Content-Type", "application/x-apple-binary-plist");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: vec![],
    };

    assert!(response.is_plist());

    let mut headers2 = Headers::new();
    headers2.insert("Content-Type", "text/plain");
    let response2 = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers: headers2,
        body: vec![],
    };
    assert!(!response2.is_plist());
}

#[test]
fn test_response_body_as_plist() {
    use crate::protocol::plist::PlistValue;

    // Create a simple plist
    let value = PlistValue::String("test".to_string());
    let encoded = crate::protocol::plist::encode(&value).unwrap();

    let mut headers = Headers::new();
    headers.insert("Content-Type", "application/x-apple-binary-plist");

    let response = RtspResponse {
        version: "RTSP/1.0".to_string(),
        status: StatusCode::OK,
        reason: "OK".to_string(),
        headers,
        body: encoded,
    };

    let decoded = response.body_as_plist().unwrap();
    assert_eq!(decoded.as_str(), Some("test"));
}
