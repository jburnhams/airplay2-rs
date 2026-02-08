use crate::receiver::ap2::config::{Ap2Config, Ap2ConfigBuilder, ConfigError};

#[test]
fn test_default_config() {
    let config = Ap2Config::default();
    assert_eq!(config.name, "AirPlay Receiver");
    assert_eq!(config.server_port, 7000);
    assert_eq!(config.max_sessions, 1);
    assert!(config.multi_room_enabled);
}

#[test]
fn test_builder_basic() {
    let config = Ap2ConfigBuilder::new()
        .name("My Speaker")
        .port(1234)
        .build()
        .expect("Failed to build config");

    assert_eq!(config.name, "My Speaker");
    assert_eq!(config.server_port, 1234);
}

#[test]
fn test_builder_invalid_name() {
    let result = Ap2ConfigBuilder::new().name("").build();
    assert!(matches!(result, Err(ConfigError::InvalidName(_))));
}

#[test]
fn test_builder_invalid_port() {
    let result = Ap2ConfigBuilder::new().port(0).build();
    assert!(matches!(result, Err(ConfigError::InvalidPort(_))));
}

#[test]
fn test_feature_flags() {
    let config = Ap2Config::default();
    let flags = config.feature_flags();

    // Check core bits
    assert_eq!(flags & (1 << 0), 1 << 0); // Video
    assert_eq!(flags & (1 << 7), 1 << 7); // Audio

    // Check multi-room bits
    assert_eq!(flags & (1 << 40), 1 << 40); // Buffered audio

    let config_no_mr = Ap2ConfigBuilder::new()
        .name("NoMR")
        .build()
        .unwrap()
        .without_multi_room();

    let flags_no_mr = config_no_mr.feature_flags();
    assert_eq!(flags_no_mr & (1 << 40), 0);
}
