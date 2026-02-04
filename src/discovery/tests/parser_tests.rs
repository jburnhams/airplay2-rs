use crate::discovery::parser::{self, feature_bits};
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

#[test]
fn test_parse_txt_records_edge_cases() {
    let records = vec![
        "key1=val1".to_string(),
        "key2=".to_string(), // Empty value
        "key3".to_string(),  // Missing equals
        "=".to_string(),     // Empty key and value
        "=val".to_string(),  // Empty key
    ];

    let parsed = parser::parse_txt_records(&records);

    assert_eq!(parsed.get("key1"), Some(&"val1".to_string()));
    assert_eq!(parsed.get("key2"), Some(&String::new()));
    // "key3" -> parts: ["key3"] -> key="key3", val=""
    assert_eq!(parsed.get("key3"), Some(&String::new()));

    // "=".to_string() -> key="", value=""
    // "=val" -> key="", value="val"
    // The last one overwrites.
    assert!(parsed.contains_key(""));
    // Depends on iteration order or if it's deterministic. Vec iteration is deterministic.
    // "=" comes first, then "=val". So empty key should have "val".
    assert_eq!(parsed.get(""), Some(&"val".to_string()));
}

#[test]
fn test_parse_features_malformed() {
    assert!(parser::parse_features("invalid").is_none());
    assert!(parser::parse_features("0xGG").is_none());
    assert!(parser::parse_features("").is_none());

    // Partial valid
    assert!(parser::parse_features("0x1,invalid").is_none());

    // Extra spaces
    let caps = parser::parse_features(" 0x1 ").unwrap();
    assert_eq!(caps.raw_features, 1);
}

#[test]
fn test_parse_features_large_values() {
    // Max u64
    let max_hex = format!("0x{:X}", u64::MAX);
    let caps = parser::parse_features(&max_hex).unwrap();
    assert_eq!(caps.raw_features, u64::MAX);
}
