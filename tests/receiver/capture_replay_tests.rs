//! Tests using captured real traffic

use std::path::Path;

use airplay2::protocol::rtsp::server_codec::RtspServerCodec;
use airplay2::receiver::ap2::request_router::{Ap2Endpoint, Ap2RequestType};
use airplay2::testing::packet_capture::{CaptureLoader, CaptureProtocol, CaptureReplay};

/// Test parsing real /info response capture
#[test]
fn test_captured_info_request() {
    let capture_path = Path::new("tests/captures/info_request.hex");

    if !capture_path.exists() {
        eprintln!("Skipping: capture file not found");
        return;
    }

    let packets = CaptureLoader::load_hex_dump(capture_path).unwrap();
    let mut replay = CaptureReplay::new(packets);

    // Get first inbound packet (should be GET /info)
    let packet = replay.next_inbound().unwrap();
    assert_eq!(packet.protocol, CaptureProtocol::Tcp);

    // Parse with RTSP codec
    let mut codec = RtspServerCodec::new();
    codec.feed(&packet.data);

    let request = codec.decode().unwrap().expect("Failed to decode request");
    let request_type = Ap2RequestType::classify(&request);
    // Verify classification
    assert!(matches!(
        request_type,
        Ap2RequestType::Endpoint(Ap2Endpoint::Info)
    ));
}

/// Test parsing real pairing exchange capture
#[test]
fn test_captured_pairing() {
    let capture_path = Path::new("tests/captures/pairing_exchange.hex");

    if !capture_path.exists() {
        eprintln!("Skipping: capture file not found");
        return;
    }

    let packets = CaptureLoader::load_hex_dump(capture_path).unwrap();

    let mut found_post = false;
    // Process entire exchange
    for packet in &packets {
        if packet.inbound {
            // Very simple validation that we can process the capture as strings
            // (real verification would involve full pairing state machine which is complex here)
            let data_str = String::from_utf8_lossy(&packet.data);
            if data_str.contains("POST /pair-setup") || data_str.contains("POST /pair-verify") {
                found_post = true;
            }
        }
    }

    assert!(
        found_post,
        "Pairing capture did not contain pairing POST requests"
    );
}

/// Template for creating new capture test
#[test]
fn test_capture_file_format() {
    // Example capture file format:
    //
    // # Comment line
    // 0 IN TCP 4f5054494f4e53...
    // 1000 OUT TCP 525453502f312e30...
    // 2000 IN TCP 47455420...
    //
    // Fields: timestamp_us direction protocol hex_data

    let example_data = "# Test capture\n0 IN TCP 4f5054494f4e53202a20525453502f312e300d0a\n1000 \
                        OUT TCP 525453502f312e3020323030204f4b0d0a\n";

    use std::io::Write;
    let mut temp = tempfile::NamedTempFile::new().unwrap();
    write!(temp, "{}", example_data).unwrap();

    let packets = CaptureLoader::load_hex_dump(temp.path()).unwrap();
    assert_eq!(packets.len(), 2);
    assert!(packets[0].inbound);
    assert!(!packets[1].inbound);
}
