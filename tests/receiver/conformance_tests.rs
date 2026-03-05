//! Protocol Conformance Tests
//!
//! Validates that our receiver correctly implements the AirPlay 2 protocol.

use airplay2::receiver::ap2::{
    capabilities::DeviceCapabilities,
    setup_handler::SetupRequest,
    stream::StreamType,
    pairing_server::PairingServer,
};
use airplay2::protocol::plist::PlistValue;
use airplay2::protocol::pairing::tlv::{TlvEncoder, TlvDecoder, TlvType};
use std::collections::HashMap;

/// Test /info response contains required fields
#[test]
fn test_info_required_fields() {
    let caps = DeviceCapabilities::audio_receiver(
        "AA:BB:CC:DD:EE:FF",
        "Test Speaker",
        [0u8; 32],
    );

    let plist = caps.to_plist();
    let dict = match plist {
        PlistValue::Dictionary(d) => d,
        _ => panic!("Expected dict"),
    };

    // Required fields per protocol
    let required = [
        "deviceid", "name", "model", "features",
        "statusFlags", "pk", "pi", "protovers", "srcvers"
    ];

    for field in &required {
        assert!(dict.contains_key(*field),
            "Missing required field: {}", field);
    }
}

/// Test feature flags are valid
#[test]
fn test_feature_flags_valid() {
    let caps = DeviceCapabilities::audio_receiver(
        "AA:BB:CC:DD:EE:FF",
        "Test Speaker",
        [0u8; 32],
    );

    let features = caps.features;

    // Audio feature (bit 9) must be set for audio receiver
    assert!(features & (1 << 9) != 0, "Audio feature bit must be set");

    // HomeKit (bit 46) should be set if we support pairing
    if caps.supports_homekit {
        assert!(features & (1 << 46) != 0, "HomeKit bit should match capability");
    }
}

/// Test SETUP request parsing for phase 1
#[test]
fn test_setup_phase1_parsing() {
    // Simulated phase 1 SETUP body
    let mut streams_dict = HashMap::new();
    streams_dict.insert("type".to_string(), PlistValue::Integer(130));  // Event

    let mut body_dict = HashMap::new();
    body_dict.insert("streams".to_string(),
        PlistValue::Array(vec![PlistValue::Dictionary(streams_dict)]));
    body_dict.insert("timingProtocol".to_string(),
        PlistValue::String("PTP".into()));

    let plist = PlistValue::Dictionary(body_dict);

    // Encode to bplist
    let body = airplay2::protocol::plist::encode(&plist).unwrap();

    // Parse
    let setup = SetupRequest::parse(&body).unwrap();

    assert!(setup.is_phase1());
    assert!(!setup.is_phase2());
    assert!(setup.streams.iter().any(|s| s.stream_type == StreamType::Event));
}

/// Test SETUP request parsing for phase 2
#[test]
fn test_setup_phase2_parsing() {
    // Simulated phase 2 SETUP body
    let mut streams_dict = HashMap::new();
    streams_dict.insert("type".to_string(), PlistValue::Integer(96));  // Audio
    streams_dict.insert("ct".to_string(), PlistValue::Integer(100));   // PCM
    streams_dict.insert("sr".to_string(), PlistValue::Integer(44100));
    streams_dict.insert("ch".to_string(), PlistValue::Integer(2));

    let mut body_dict = HashMap::new();
    body_dict.insert("streams".to_string(),
        PlistValue::Array(vec![PlistValue::Dictionary(streams_dict)]));
    body_dict.insert("et".to_string(), PlistValue::Integer(4));  // ChaCha20
    body_dict.insert("shk".to_string(), PlistValue::Data(vec![0u8; 32]));

    let plist = PlistValue::Dictionary(body_dict);
    let body = airplay2::protocol::plist::encode(&plist).unwrap();

    let setup = SetupRequest::parse(&body).unwrap();

    assert!(!setup.is_phase1());
    assert!(setup.is_phase2());

    let audio_stream = setup.streams.iter()
        .find(|s| s.stream_type == StreamType::Audio)
        .expect("Should have audio stream");

    let format = audio_stream.audio_format.as_ref().unwrap();
    assert_eq!(format.codec, 100);
    assert_eq!(format.sample_rate, 44100);
}

/// Test pairing TLV encoding
#[test]
fn test_pairing_tlv_format() {
    // Build M1 message
    let m1 = TlvEncoder::new()
        .add_state(1)   // State = 1
        .add_byte(TlvType::Method, 0)   // Method = 0 (pair-setup)
        .build();

    // Parse back
    let decoded = TlvDecoder::decode(&m1).unwrap();

    assert_eq!(decoded.get_state().ok(), Some(1));
    assert_eq!(decoded.get(TlvType::Method), Some(b"\0".as_ref()));
}

/// Test pairing state machine rejects out-of-order messages
#[test]
fn test_pairing_state_machine() {
    use airplay2::protocol::crypto::Ed25519KeyPair;

    let identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");

    // Try M3 before M1 - should fail
    let m3 = TlvEncoder::new()
        .add_state(3)
        .build();

    let result = server.process_pair_setup(&m3);
    assert!(result.error.is_some(), "Should reject M3 before M1");
}
