use crate::streaming::url::{PlaybackInfo, UrlStreamer};

#[tokio::test]
async fn test_url_streamer_creation() {
    use crate::connection::ConnectionManager;
    use crate::types::AirPlayConfig;
    use std::sync::Arc;

    let config = AirPlayConfig::default();
    let connection = Arc::new(ConnectionManager::new(config));

    let streamer = UrlStreamer::new(connection);
    assert!(!streamer.is_playing());
}

#[test]
#[allow(clippy::float_cmp)]
fn test_parse_playback_info() {
    use crate::plist_dict;
    // Construct a sample plist dictionary
    let dict = plist_dict![
        "position" => 10.5,
        "duration" => 120.0,
        "rate" => 1.0,
        "readyToPlay" => true,
        "playbackBufferEmpty" => false
    ];

    let data = crate::protocol::plist::encode(&dict).unwrap();

    let info = UrlStreamer::parse_playback_info(&data).unwrap();

    assert_eq!(info.position, 10.5);
    assert_eq!(info.duration, 120.0);
    assert_eq!(info.rate, 1.0);
    assert!(info.playing);
    assert!(info.ready_to_play);
    assert!(!info.playback_buffer_empty);
}

#[test]
#[allow(clippy::float_cmp)]
fn test_playback_info_defaults() {
    let info = PlaybackInfo {
        position: 0.0,
        duration: 100.0,
        rate: 1.0,
        playing: true,
        ready_to_play: true,
        playback_buffer_empty: false,
        loaded_time_ranges: Vec::new(),
        seekable_time_ranges: Vec::new(),
    };

    assert!(info.playing);
    assert_eq!(info.duration, 100.0);
}
