use crate::protocol::plist::PlistValue;
use crate::receiver::ap2::setup_handler::*;
use crate::receiver::ap2::stream::{AudioStreamFormat, EncryptionType, StreamType, TimingProtocol};

#[test]
fn test_phase_detection() {
    // Phase 1 request
    let phase1 = SetupRequest {
        streams: vec![
            StreamRequest {
                stream_type: StreamType::Event,
                control_port: None,
                data_port: None,
                audio_format: None,
                sender_address: None,
            },
            StreamRequest {
                stream_type: StreamType::Timing,
                control_port: None,
                data_port: None,
                audio_format: None,
                sender_address: None,
            },
        ],
        timing_protocol: TimingProtocol::Ptp,
        timing_peer_info: None,
        group_uuid: None,
        encryption_type: EncryptionType::None,
        shared_key: None,
    };

    assert!(phase1.is_phase1());
    assert!(!phase1.is_phase2());

    // Phase 2 request
    let phase2 = SetupRequest {
        streams: vec![StreamRequest {
            stream_type: StreamType::Audio,
            control_port: Some(6001),
            data_port: Some(6000),
            audio_format: Some(AudioStreamFormat {
                codec: 96,
                sample_rate: 44100,
                channels: 2,
                bits_per_sample: 16,
                frames_per_packet: 352,
                compression_type: None,
                spf: None,
            }),
            sender_address: None,
        }],
        timing_protocol: TimingProtocol::Ptp,
        timing_peer_info: None,
        group_uuid: None,
        encryption_type: EncryptionType::ChaCha20Poly1305,
        shared_key: Some(vec![0u8; 32]),
    };

    assert!(!phase2.is_phase1());
    assert!(phase2.is_phase2());
}

#[test]
fn test_port_allocator() {
    let mut allocator = PortAllocator::new(7000, 7010);

    let p1 = allocator.allocate().unwrap();
    let p2 = allocator.allocate().unwrap();

    assert_ne!(p1, p2);
    assert!((7000..=7010).contains(&p1));
    assert!((7000..=7010).contains(&p2));

    allocator.release(p1);

    let p3 = allocator.allocate().unwrap();
    assert!((7000..=7010).contains(&p3));
}

#[test]
fn test_response_plist() {
    let response = SetupResponse::phase1(7010, 7011);
    let plist = response.to_plist();

    if let PlistValue::Dictionary(dict) = plist {
        assert!(dict.contains_key("eventPort"));
        assert!(dict.contains_key("timingPort"));
        assert!(dict.contains_key("streams"));
    } else {
        panic!("Expected Dict");
    }
}

#[test]
fn test_setup_handler_phases() {
    let handler = SetupHandler::new(7000, 7100, 88200);

    // Simulate phase 1 check
    let phase1_request = SetupRequest {
        streams: vec![StreamRequest {
            stream_type: StreamType::Event,
            control_port: None,
            data_port: None,
            audio_format: None,
            sender_address: None,
        }],
        timing_protocol: TimingProtocol::Ptp,
        timing_peer_info: None,
        group_uuid: None,
        encryption_type: EncryptionType::None,
        shared_key: None,
    };

    assert!(phase1_request.is_phase1());

    // After cleanup
    handler.cleanup();

    // Check if phase is reset to None
    let phase = handler.current_phase.lock().unwrap();
    assert!(matches!(*phase, SetupPhase::None));
}
