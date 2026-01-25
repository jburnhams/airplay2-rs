use airplay2::protocol::plist;
use airplay2::protocol::rtsp::{RtspCodec, StatusCode};

#[test]
fn test_rtsp_plist_integration() {
    // 1. Create a binary plist
    let track_info = airplay2::plist_dict! {
        "title" => "Integration Test",
        "duration" => 123.456,
        "is_playing" => true,
    };

    // 2. Encode plist to bytes
    let plist_bytes = plist::encode(&track_info).unwrap();

    // 3. Simulate RTSP Response containing this plist
    let header = format!(
        "RTSP/1.0 200 OK\r\nCSeq: 10\r\nContent-Type: \
         application/x-apple-binary-plist\r\nContent-Length: {}\r\n\r\n",
        plist_bytes.len()
    );

    let mut response_bytes = header.into_bytes();
    response_bytes.extend_from_slice(&plist_bytes);

    // 4. Decode using RtspCodec
    let mut codec = RtspCodec::new();
    codec.feed(&response_bytes).unwrap();
    let response = codec.decode().unwrap().unwrap();

    // 5. Verify response properties
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.is_plist());

    // 6. Decode body using helper
    let decoded_plist = response.body_as_plist().unwrap();

    // 7. Verify content
    let dict = decoded_plist.as_dict().unwrap();
    assert_eq!(
        dict.get("title").and_then(|v| v.as_str()),
        Some("Integration Test")
    );
    assert_eq!(dict.get("is_playing").and_then(|v| v.as_bool()), Some(true));
    match dict.get("duration") {
        Some(airplay2::protocol::plist::PlistValue::Real(d)) => {
            assert!((d - 123.456).abs() < f64::EPSILON);
        }
        _ => panic!("Expected duration real"),
    }
}

#[test]
fn test_rtsp_plist_fragmented_integration() {
    let track_info = airplay2::plist_dict! {
        "long_string" => "A".repeat(1000),
    };
    let plist_bytes = plist::encode(&track_info).unwrap();

    let header = format!(
        "RTSP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n",
        plist_bytes.len()
    );

    let mut full_data = header.into_bytes();
    full_data.extend_from_slice(&plist_bytes);

    let mut codec = RtspCodec::new();

    // Split data into chunks
    for chunk in full_data.chunks(100) {
        codec.feed(chunk).unwrap();
        if let Some(response) = codec.decode().unwrap() {
            let decoded = response.body_as_plist().unwrap();
            let dict = decoded.as_dict().unwrap();
            assert_eq!(
                dict.get("long_string")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len()),
                Some(1000)
            );
            return;
        }
    }
    panic!("Failed to decode fragmented response");
}
