use crate::receiver::ap2::config::{Ap2Config, Ap2ConfigBuilder, ConfigError};
use crate::types::RaopCodec;

#[test]
fn test_default_config() {
    let config = Ap2Config::default();
    assert_eq!(config.name, "AirPlay Receiver");
    assert_eq!(config.server_port, 7000);
    assert!(config.multi_room_enabled);
    assert!(config.audio_formats.contains(&RaopCodec::Pcm));
}

#[test]
fn test_feature_flags() {
    let config = Ap2Config::default();
    let flags = config.feature_flags();

    // Check required bits
    assert_eq!(flags & (1 << 0), 1 << 0, "Video supported bit missing");
    assert_eq!(flags & (1 << 7), 1 << 7, "Audio supported bit missing");

    // Check multi-room bits
    assert_eq!(flags & (1 << 40), 1 << 40, "Buffered audio bit missing");
}

#[test]
fn test_builder() {
    let config = Ap2ConfigBuilder::new()
        .name("My Speaker")
        .port(5000)
        .build()
        .expect("Failed to build config");

    assert_eq!(config.name, "My Speaker");
    assert_eq!(config.server_port, 5000);
}

#[test]
fn test_builder_validation() {
    let result = Ap2ConfigBuilder::new().name("").build();

    match result {
        Err(ConfigError::InvalidName(_)) => (),
        _ => panic!("Expected InvalidName error"),
    }
}
