use super::parser::{self, feature_bits};
use crate::types::DeviceCapabilities;

#[test]
fn test_parse_txt_records() {
    let records = vec![
        "key1=value1".to_string(),
        "key2=value2".to_string(),
        "key3=".to_string(),
    ];

    let parsed = parser::parse_txt_records(&records);

    assert_eq!(parsed.get("key1"), Some(&"value1".to_string()));
    assert_eq!(parsed.get("key2"), Some(&"value2".to_string()));
    assert_eq!(parsed.get("key3"), Some(&String::new()));
}

#[test]
fn test_feature_bit_audio() {
    let features = feature_bits::AUDIO;
    let caps = DeviceCapabilities::from_features(features);
    assert!(caps.supports_audio);
}

#[test]
fn test_feature_bit_airplay2() {
    let features = feature_bits::AIRPLAY_2 | feature_bits::AUDIO;
    let caps = DeviceCapabilities::from_features(features);
    assert!(caps.airplay2);
    assert!(caps.supports_audio);
}

#[test]
fn test_feature_bit_grouping() {
    let features = feature_bits::UNIFIED_MEDIA_CONTROL;
    let caps = DeviceCapabilities::from_features(features);
    assert!(caps.supports_grouping);
}

#[test]
fn test_parse_hex_simple() {
    // We cannot access parse_hex directly as it is private, but we can test via parse_features
    let caps = parser::parse_features("0x1234").unwrap();
    assert_eq!(caps.raw_features, 0x1234);

    let caps = parser::parse_features("1234").unwrap();
    assert_eq!(caps.raw_features, 0x1234);

    let caps = parser::parse_features("0X1234").unwrap();
    assert_eq!(caps.raw_features, 0x1234);
}

#[test]
fn test_parse_features_single() {
    let caps = parser::parse_features("0x1C340405F8A00").unwrap();
    assert!(caps.supports_audio);
}

#[test]
fn test_parse_features_comma() {
    let caps = parser::parse_features("0x1C340,0x405F8A00").unwrap();
    // Check that features from both parts are combined. Format is low,high.
    // So 0x1C340 is low, 0x405F8A00 is high.
    // 0x405F8A00 << 32 | 0x1C340
    let expected = (0x405F_8A00_u64 << 32) | 0x1C340_u64;
    assert_eq!(caps.raw_features, expected);
}

#[test]
fn test_parse_model_name() {
    assert_eq!(
        parser::parse_model_name("AudioAccessory5,1"),
        "HomePod mini"
    );
    assert_eq!(parser::parse_model_name("Unknown"), "Unknown");
}

#[tokio::test]
async fn test_scan_with_timeout() {
    use super::scan;
    use std::time::Duration;

    // This test attempts to scan. It should not fail, but may return empty list.
    let result = scan(Duration::from_millis(100)).await;
    assert!(result.is_ok());
}
