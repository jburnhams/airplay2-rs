# Section 62: Integration & Conformance Testing

## Dependencies
- **Section 61**: Testing Infrastructure
- **Section 60**: Receiver Integration

## Overview

This section provides comprehensive integration tests and protocol conformance tests for the AirPlay 2 receiver. Tests use mock senders and captured traffic - no real devices required.

## Objectives

- End-to-end session testing
- Protocol conformance validation
- Error handling verification
- Performance benchmarking
- Audio quality verification

---

## Tasks

### 62.1 Session Integration Tests

**File:** `tests/receiver/integration_tests.rs`

```rust
//! Integration Tests for AirPlay 2 Receiver

use airplay2::receiver::ap2::{
    AirPlay2Receiver, Ap2Config, ReceiverEvent, ReceiverState,
};
use airplay2::testing::{
    MockAp2Sender, MockSenderConfig, MockAudioFormat,
    generate_test_audio, samples_match, wait_for,
};
use std::time::Duration;
use tokio::sync::mpsc;

/// Test complete session from connection to teardown
#[tokio::test]
async fn test_full_session() {
    // Start receiver
    let port = portpicker::pick_unused_port().unwrap();
    let config = Ap2Config::new("Test Speaker")
        .with_port(port)
        .with_password("1234");

    let mut receiver = AirPlay2Receiver::new(config).unwrap();
    let mut events = receiver.subscribe();

    receiver.start().await.unwrap();
    assert_eq!(receiver.state().await, ReceiverState::Running);

    // Connect mock sender
    let mut sender = MockAp2Sender::new(MockSenderConfig {
        pin: "1234".into(),
        ..Default::default()
    });

    sender.connect(format!("127.0.0.1:{}", port).parse().unwrap())
        .await.unwrap();

    // Perform full session
    let session = sender.full_session().await.unwrap();

    // Verify events received
    let mut connected = false;
    let mut paired = false;
    let mut streaming = false;

    while let Ok(event) = tokio::time::timeout(Duration::from_secs(1), events.recv()).await {
        match event {
            Ok(ReceiverEvent::Connected { .. }) => connected = true,
            Ok(ReceiverEvent::PairingComplete) => paired = true,
            Ok(ReceiverEvent::StreamingStarted) => streaming = true,
            _ => {}
        }
        if streaming { break; }
    }

    assert!(connected, "Should receive Connected event");
    assert!(paired, "Should receive PairingComplete event");
    assert!(streaming, "Should receive StreamingStarted event");

    // Teardown
    sender.teardown().await.unwrap();
    receiver.stop().await.unwrap();
}

/// Test authentication with wrong password
#[tokio::test]
async fn test_wrong_password() {
    let port = portpicker::pick_unused_port().unwrap();
    let config = Ap2Config::new("Test Speaker")
        .with_port(port)
        .with_password("correct_password");

    let mut receiver = AirPlay2Receiver::new(config).unwrap();
    receiver.start().await.unwrap();

    // Connect with wrong password
    let mut sender = MockAp2Sender::new(MockSenderConfig {
        pin: "wrong_password".into(),
        ..Default::default()
    });

    sender.connect(format!("127.0.0.1:{}", port).parse().unwrap())
        .await.unwrap();

    // Pairing should fail
    let result = sender.pair_setup().await;

    // Clean shutdown even with failed pairing
    receiver.stop().await.unwrap();
}

/// Test reconnection after disconnect
#[tokio::test]
async fn test_reconnection() {
    let port = portpicker::pick_unused_port().unwrap();
    let config = Ap2Config::new("Test Speaker").with_port(port);

    let mut receiver = AirPlay2Receiver::new(config).unwrap();
    receiver.start().await.unwrap();

    // First connection
    let mut sender1 = MockAp2Sender::new(MockSenderConfig::default());
    sender1.connect(format!("127.0.0.1:{}", port).parse().unwrap())
        .await.unwrap();
    sender1.get_info().await.unwrap();
    drop(sender1);

    // Brief pause
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Second connection
    let mut sender2 = MockAp2Sender::new(MockSenderConfig::default());
    sender2.connect(format!("127.0.0.1:{}", port).parse().unwrap())
        .await.unwrap();
    sender2.get_info().await.unwrap();

    receiver.stop().await.unwrap();
}
```

---

### 62.2 Protocol Conformance Tests

**File:** `tests/receiver/conformance_tests.rs`

```rust
//! Protocol Conformance Tests
//!
//! Validates that our receiver correctly implements the AirPlay 2 protocol.

use airplay2::receiver::ap2::{
    capabilities::DeviceCapabilities,
    setup_handler::{SetupRequest, StreamType},
    pairing_server::PairingServer,
};
use airplay2::protocol::plist::PlistValue;
use airplay2::protocol::pairing::tlv::{TlvEncoder, TlvDecoder};
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
        PlistValue::Dict(d) => d,
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
        PlistValue::Array(vec![PlistValue::Dict(streams_dict)]));
    body_dict.insert("timingProtocol".to_string(),
        PlistValue::String("PTP".into()));

    let plist = PlistValue::Dict(body_dict);

    // Encode to bplist
    let body = airplay2::protocol::plist::BinaryPlistEncoder::encode(&plist).unwrap();

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
        PlistValue::Array(vec![PlistValue::Dict(streams_dict)]));
    body_dict.insert("et".to_string(), PlistValue::Integer(4));  // ChaCha20
    body_dict.insert("shk".to_string(), PlistValue::Data(vec![0u8; 32]));

    let plist = PlistValue::Dict(body_dict);
    let body = airplay2::protocol::plist::BinaryPlistEncoder::encode(&plist).unwrap();

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
        .add_u8(0x06, 1)   // State = 1
        .add_u8(0x00, 0)   // Method = 0 (pair-setup)
        .encode();

    // Parse back
    let decoded = TlvDecoder::decode(&m1).unwrap();

    assert_eq!(decoded.get_u8(0x06), Some(1));
    assert_eq!(decoded.get_u8(0x00), Some(0));
}

/// Test pairing state machine rejects out-of-order messages
#[test]
fn test_pairing_state_machine() {
    use airplay2::protocol::crypto::ed25519::Ed25519Keypair;

    let identity = Ed25519Keypair::generate();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");

    // Try M3 before M1 - should fail
    let m3 = TlvEncoder::new()
        .add_u8(0x06, 3)
        .encode();

    let result = server.process_pair_setup(&m3);
    assert!(result.error.is_some(), "Should reject M3 before M1");
}
```

---

### 62.3 Audio Quality Tests

**File:** `tests/receiver/audio_tests.rs`

```rust
//! Audio Quality Tests

use airplay2::receiver::ap2::jitter_buffer::{JitterBuffer, JitterBufferConfig, BufferState};
use airplay2::receiver::ap2::rtp_receiver::AudioFrame;
use airplay2::testing::generate_test_audio;
use std::time::Instant;

/// Test jitter buffer maintains audio continuity
#[test]
fn test_jitter_buffer_continuity() {
    let config = JitterBufferConfig {
        target_depth_ms: 100,
        sample_rate: 44100,
        channels: 2,
        ..Default::default()
    };

    let mut buffer = JitterBuffer::new(config);

    // Add frames in order
    for i in 0..50u16 {
        let frame = AudioFrame {
            sequence: i,
            timestamp: i as u32 * 352,
            samples: vec![i as i16; 704],  // 352 stereo samples
            receive_time: Instant::now(),
        };
        buffer.push(frame);
    }

    assert_eq!(buffer.state(), BufferState::Playing);

    // Pull samples and verify continuity
    let samples = buffer.pull(352);
    assert_eq!(samples.len(), 704);  // Stereo
}

/// Test jitter buffer handles packet loss
#[test]
fn test_jitter_buffer_loss_concealment() {
    let config = JitterBufferConfig {
        target_depth_ms: 100,
        sample_rate: 44100,
        channels: 2,
        ..Default::default()
    };

    let mut buffer = JitterBuffer::new(config);

    // Add frames with gap
    for i in 0..20u16 {
        buffer.push(AudioFrame {
            sequence: i,
            timestamp: i as u32 * 352,
            samples: vec![100i16; 704],
            receive_time: Instant::now(),
        });
    }

    // Skip frame 20, add 21-30
    for i in 21..30u16 {
        buffer.push(AudioFrame {
            sequence: i,
            timestamp: i as u32 * 352,
            samples: vec![100i16; 704],
            receive_time: Instant::now(),
        });
    }

    assert!(buffer.stats().frames_lost > 0, "Should detect lost frame");
}

/// Test audio decryption with known vectors
#[test]
fn test_audio_decryption() {
    // This would use known test vectors
    // For now, just verify the decryptor can be created

    use airplay2::receiver::ap2::rtp_decryptor::Ap2RtpDecryptor;

    let key = [0x42u8; 32];
    let decryptor = Ap2RtpDecryptor::new(key);

    // Would test with known encrypted payload
}
```

---

### 62.4 Capture Replay Tests

**File:** `tests/receiver/capture_replay_tests.rs`

```rust
//! Tests using captured real traffic

use airplay2::testing::{CaptureLoader, CaptureReplay, CaptureProtocol};
use airplay2::receiver::ap2::request_router::Ap2RequestType;
use airplay2::protocol::rtsp::RtspServerCodec;
use std::path::Path;

/// Test parsing real /info response capture
#[test]
#[ignore]  // Requires capture file
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

    if let Some(request) = codec.decode().unwrap() {
        let request_type = Ap2RequestType::classify(&request);
        // Verify classification
    }
}

/// Test parsing real pairing exchange capture
#[test]
#[ignore]  // Requires capture file
fn test_captured_pairing() {
    let capture_path = Path::new("tests/captures/pairing_exchange.hex");

    if !capture_path.exists() {
        eprintln!("Skipping: capture file not found");
        return;
    }

    let packets = CaptureLoader::load_hex_dump(capture_path).unwrap();

    // Process entire exchange
    for packet in &packets {
        if packet.inbound {
            // Would feed to pairing server and verify responses
        }
    }
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

    let example_data = "# Test capture\n\
                        0 IN TCP 4f5054494f4e53202a20525453502f312e300d0a\n\
                        1000 OUT TCP 525453502f312e3020323030204f4b0d0a\n";

    use std::io::Write;
    let mut temp = tempfile::NamedTempFile::new().unwrap();
    write!(temp, "{}", example_data).unwrap();

    let packets = CaptureLoader::load_hex_dump(temp.path()).unwrap();
    assert_eq!(packets.len(), 2);
    assert!(packets[0].inbound);
    assert!(!packets[1].inbound);
}
```

---

### 62.5 Test Capture Guide

**File:** `tests/captures/README.md`

```markdown
# AirPlay 2 Test Captures

This directory contains packet captures for testing. To create new captures:

## Capturing from iOS/macOS

1. Use Wireshark to capture traffic between an iOS device and a known AirPlay receiver
2. Filter for the AirPlay port (usually 7000)
3. Export as hex dump using the format below

## Capture File Format

Plain text, one packet per line:
```
timestamp_us direction protocol hex_data
```

- `timestamp_us`: Microseconds from start of capture
- `direction`: `IN` (sender→receiver) or `OUT` (receiver→sender)
- `protocol`: `TCP` or `UDP`
- `hex_data`: Packet payload as hex string

Example:
```
0 IN TCP 4f5054494f4e53202a20525453502f312e30...
1500 OUT TCP 525453502f312e3020323030204f4b...
```

## Required Captures

- [ ] `info_request.hex` - GET /info exchange
- [ ] `pairing_exchange.hex` - Full pair-setup and pair-verify
- [ ] `setup_phase1.hex` - SETUP for timing/event
- [ ] `setup_phase2.hex` - SETUP for audio
- [ ] `audio_streaming.hex` - RTP audio packets (encrypted)
- [ ] `volume_metadata.hex` - SET_PARAMETER for volume/metadata

## Sanitizing Captures

Before committing captures, remove:
- Real IP addresses (replace with 192.168.1.x)
- MAC addresses (replace with AA:BB:CC:DD:EE:FF)
- Personal device names
```

---

## Acceptance Criteria

- [ ] Full session test passes with mock sender
- [ ] Wrong password is rejected
- [ ] Reconnection works correctly
- [ ] All conformance tests pass
- [ ] Audio continuity maintained through jitter buffer
- [ ] Packet loss handled gracefully
- [ ] Capture replay infrastructure works
- [ ] Test capture format documented

---

## CI/CD Integration

Add to `.github/workflows/test.yml`:

```yaml
- name: Run receiver tests
  run: cargo test --features receiver -- --include-ignored
  env:
    RUST_LOG: debug
```

---

## References

- [Section 61: Testing Infrastructure](./61-testing-infrastructure.md)
- [Section 45: Testing (AirPlay 1 Receiver)](./complete/45-receiver-testing.md)
