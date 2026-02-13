mod raop;

use super::*;
use std::time::Duration;

// --- config.rs tests ---

#[test]
fn test_config_defaults() {
    let config = AirPlayConfig::default();

    assert_eq!(config.discovery_timeout, Duration::from_secs(5));
    assert_eq!(config.connection_timeout, Duration::from_secs(10));
    assert_eq!(config.state_poll_interval, Duration::from_millis(500));
    assert!(!config.debug_protocol);
    assert_eq!(config.reconnect_attempts, 3);
    assert_eq!(config.reconnect_delay, Duration::from_secs(1));
    assert_eq!(config.audio_buffer_frames, 44100);
    assert!(config.pairing_storage_path.is_none());
}

#[test]
fn test_config_builder() {
    let path = std::path::PathBuf::from("/tmp");
    let config = AirPlayConfig::builder()
        .discovery_timeout(Duration::from_secs(10))
        .connection_timeout(Duration::from_secs(20))
        .state_poll_interval(Duration::from_secs(1))
        .debug_protocol(true)
        .pairing_storage(path.clone())
        .build();

    assert_eq!(config.discovery_timeout, Duration::from_secs(10));
    assert_eq!(config.connection_timeout, Duration::from_secs(20));
    assert_eq!(config.state_poll_interval, Duration::from_secs(1));
    assert!(config.debug_protocol);
    assert_eq!(config.pairing_storage_path, Some(path));
}

// --- device.rs tests ---

#[test]
fn test_device_capabilities_from_features_homepod_mini() {
    // Known HomePod Mini features value
    let features = 0x0001_C340_405F_8A00;
    let caps = DeviceCapabilities::from_features(features);

    assert!(caps.supports_audio);
    assert!(caps.airplay2);
    assert!(caps.supports_buffered_audio);
    // Bit 32 is not set in this specific feature mask
    assert!(!caps.supports_grouping);
    assert_eq!(caps.raw_features, features);
}

#[test]
fn test_device_capabilities_grouping() {
    // Set Bit 32 explicitly
    let features = 1u64 << 32;
    let caps = DeviceCapabilities::from_features(features);
    assert!(caps.supports_grouping);
}

#[test]
fn test_device_capabilities_empty() {
    let caps = DeviceCapabilities::from_features(0);

    assert!(!caps.supports_audio);
    assert!(!caps.airplay2);
    assert!(!caps.supports_grouping);
    assert_eq!(caps.raw_features, 0);
}

#[test]
fn test_device_capabilities_all_flags() {
    let features = u64::MAX;
    let caps = DeviceCapabilities::from_features(features);

    assert!(caps.supports_audio);
    assert!(caps.airplay2);
    assert!(caps.supports_grouping);
}

#[test]
fn test_airplay_device_methods() {
    let caps = DeviceCapabilities {
        airplay2: true,
        supports_grouping: false,
        ..Default::default()
    };

    let device = AirPlayDevice {
        id: "id".to_string(),
        name: "name".to_string(),
        model: None,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 7000,
        capabilities: caps,
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    assert!(device.supports_airplay2());
    assert!(!device.supports_grouping());
    assert!(device.discovered_volume().is_none());
}

#[test]
fn test_device_discovered_volume() {
    let mut txt = std::collections::HashMap::new();
    txt.insert("vv".to_string(), "2".to_string()); // Does 2 mean something? Usually floats?

    // Assuming "vv" parses as f32?
    // Implementation uses parse().ok().

    let device = AirPlayDevice {
        id: "id".to_string(),
        name: "name".to_string(),
        model: None,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 7000,
        capabilities: DeviceCapabilities::default(),
        raop_port: None,
        raop_capabilities: None,
        txt_records: txt,
    };

    assert_eq!(device.discovered_volume(), Some(2.0));
}

// --- PTP / TimingProtocol tests ---

#[test]
fn test_timing_protocol_default_is_auto() {
    let tp = TimingProtocol::default();
    assert_eq!(tp, TimingProtocol::Auto);
}

#[test]
fn test_config_default_timing_protocol() {
    let config = AirPlayConfig::default();
    assert_eq!(config.timing_protocol, TimingProtocol::Auto);
}

#[test]
fn test_config_builder_timing_protocol() {
    let config = AirPlayConfig::builder()
        .timing_protocol(TimingProtocol::Ptp)
        .build();
    assert_eq!(config.timing_protocol, TimingProtocol::Ptp);

    let config = AirPlayConfig::builder()
        .timing_protocol(TimingProtocol::Ntp)
        .build();
    assert_eq!(config.timing_protocol, TimingProtocol::Ntp);
}

#[test]
fn test_device_supports_ptp_from_feature_bit_40() {
    // Bit 40 set
    let features = 1u64 << 40;
    let caps = DeviceCapabilities::from_features(features);
    assert!(caps.supports_ptp);
}

#[test]
fn test_device_supports_ptp_not_set() {
    let caps = DeviceCapabilities::from_features(0);
    assert!(!caps.supports_ptp);
}

#[test]
fn test_device_supports_ptp_method() {
    let caps = DeviceCapabilities {
        supports_ptp: true,
        ..Default::default()
    };
    let device = AirPlayDevice {
        id: "id".to_string(),
        name: "name".to_string(),
        model: None,
        addresses: vec!["192.168.1.1".parse().unwrap()],
        port: 7000,
        capabilities: caps,
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };
    assert!(device.supports_ptp());
}

#[test]
fn test_device_ptp_flag_in_all_features() {
    let caps = DeviceCapabilities::from_features(u64::MAX);
    assert!(caps.supports_ptp);
}

#[test]
fn test_device_supports_ptp_typical_homepod() {
    // HomePod features: typically has bits 9 (audio), 38 (buffered), 40 (PTP), 48 (AirPlay2)
    let features = (1u64 << 9) | (1u64 << 38) | (1u64 << 40) | (1u64 << 48);
    let caps = DeviceCapabilities::from_features(features);
    assert!(caps.supports_audio);
    assert!(caps.supports_buffered_audio);
    assert!(caps.supports_ptp);
    assert!(caps.airplay2);
}

// --- state.rs tests ---

#[test]
fn test_playback_state_default() {
    let state = PlaybackState::default();
    assert!(!state.is_playing);
    assert!(state.current_track.is_none());
    assert!((state.volume - 0.0).abs() < f32::EPSILON);
    assert_eq!(state.repeat, RepeatMode::Off);
    assert!(state.queue.is_empty());
    assert_eq!(state.connection_state, ConnectionState::Disconnected);
}

#[test]
fn test_playback_info_from_state() {
    let track = TrackInfo::new("url", "title", "artist");
    let state = PlaybackState {
        position_secs: 30.5,
        is_playing: true,
        current_track: Some(track.clone()),
        queue_index: Some(2),
        ..Default::default()
    };

    let info = PlaybackInfo::from(&state);

    assert_eq!(info.position_ms, 30500);
    assert!(info.is_playing);
    assert_eq!(info.index, 2);
    assert_eq!(info.current_track, Some(track));
}

#[test]
fn test_playback_info_from_state_queue_items() {
    let track = TrackInfo::new("url", "title", "artist");
    let queue = vec![
        QueueItem {
            id: QueueItemId(10),
            track: track.clone(),
            original_position: 0,
        },
        QueueItem {
            id: QueueItemId(20),
            track: track.clone(),
            original_position: 1,
        },
    ];

    let state = PlaybackState {
        queue,
        ..Default::default()
    };

    let info = PlaybackInfo::from(&state);
    assert_eq!(info.items.len(), 2);
    assert_eq!(info.items[0].1, 10);
    assert_eq!(info.items[1].1, 20);
}

#[test]
fn test_repeat_mode_equality() {
    assert_eq!(RepeatMode::Off, RepeatMode::Off);
    assert_ne!(RepeatMode::Off, RepeatMode::All);
    assert_ne!(RepeatMode::All, RepeatMode::One);
}

// --- track.rs tests ---

#[test]
fn test_track_info_builder() {
    let track = TrackInfo::new("http://example.com/track.mp3", "Test Track", "Test Artist")
        .with_album("Test Album")
        .with_artwork("http://art.jpg")
        .with_duration(180.5);

    assert_eq!(track.url, "http://example.com/track.mp3");
    assert_eq!(track.title, "Test Track");
    assert_eq!(track.artist, "Test Artist");
    assert_eq!(track.album, Some("Test Album".to_string()));
    assert_eq!(track.artwork_url, Some("http://art.jpg".to_string()));
    assert_eq!(track.duration_secs, Some(180.5));
}

#[test]
fn test_track_info_default() {
    let track = TrackInfo::default();
    assert!(track.url.is_empty());
    assert!(track.duration_secs.is_none());
}

#[test]
fn test_queue_item() {
    let track = TrackInfo::new("u", "t", "a");
    let item = QueueItem {
        id: QueueItemId(123),
        track: track.clone(),
        original_position: 0,
    };

    assert_eq!(item.id, QueueItemId(123));
    assert_eq!(item.track, track);
}
