use crate::receiver::ap2::features::{FeatureFlag, FeatureFlags, StatusFlag, StatusFlags};

#[test]
fn test_feature_flags_builder() {
    let mut flags = FeatureFlags::new();
    flags.set(FeatureFlag::Audio);
    flags.set(FeatureFlag::SupportsHomeKit);

    assert!(flags.has(FeatureFlag::Audio));
    assert!(flags.has(FeatureFlag::SupportsHomeKit));
    assert!(!flags.has(FeatureFlag::Video));
}

#[test]
fn test_audio_receiver_defaults() {
    let flags = FeatureFlags::audio_receiver();

    assert!(flags.has(FeatureFlag::Audio));
    assert!(flags.has(FeatureFlag::AudioFormatAlac));
    assert!(flags.has(FeatureFlag::SupportsHomeKit));
    assert!(!flags.has(FeatureFlag::Video));
}

#[test]
fn test_txt_value_roundtrip() {
    let flags = FeatureFlags::multi_room_receiver();
    let txt = flags.to_txt_value();

    let parsed = FeatureFlags::from_txt_value(&txt).unwrap();
    assert_eq!(flags.raw(), parsed.raw());
}

#[test]
fn test_status_flags() {
    let flags = StatusFlags::with_password();

    assert!(flags.has(StatusFlag::SupportsPin));
    assert!(flags.has(StatusFlag::RequiresPassword));
    assert!(!flags.has(StatusFlag::ProblemDetected));
}
