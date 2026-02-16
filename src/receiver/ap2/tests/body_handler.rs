use crate::protocol::plist::PlistValue;
use crate::receiver::ap2::body_handler::{
    BodyParseError, PlistExt, PlistResponseBuilder, encode_bplist_body, encode_text_parameters,
    parse_bplist_body, parse_text_parameters,
};
use std::collections::HashMap;

#[test]
fn test_text_parameters_roundtrip() {
    let mut params = HashMap::new();
    params.insert("volume".to_string(), "-15.0".to_string());
    params.insert("progress".to_string(), "0/44100/88200".to_string());

    let encoded = encode_text_parameters(&params);
    let decoded = parse_text_parameters(&encoded).unwrap();

    assert_eq!(decoded.get("volume"), Some(&"-15.0".to_string()));
    assert_eq!(decoded.get("progress"), Some(&"0/44100/88200".to_string()));
}

#[test]
fn test_plist_builder() {
    let plist = PlistResponseBuilder::new()
        .string("name", "Test Device")
        .int("port", 7000)
        .bool("enabled", true)
        .build();

    assert_eq!(plist.get_string("name"), Some("Test Device"));
    assert_eq!(plist.get_int("port"), Some(7000));
    assert_eq!(plist.get_bool("enabled"), Some(true));
}

#[test]
fn test_plist_types() {
    let mut dict = HashMap::new();
    dict.insert("data".to_string(), PlistValue::Data(vec![1, 2, 3]));
    dict.insert("bool".to_string(), PlistValue::Boolean(false));

    let plist = PlistValue::Dictionary(dict);

    assert_eq!(plist.get_bytes("data"), Some(&[1u8, 2, 3][..]));
    assert_eq!(plist.get_bool("bool"), Some(false));
    assert_eq!(plist.get_string("missing"), None);
}

#[test]
fn test_parse_bplist_invalid_magic() {
    let body = b"bad_magic_header";
    let result = parse_bplist_body(body);
    assert!(matches!(result, Err(BodyParseError::InvalidMagic)));
}

#[test]
fn test_parse_bplist_empty() {
    let body = b"";
    let result = parse_bplist_body(body);
    // The implementation returns an empty dictionary for empty input.
    assert!(matches!(result, Ok(PlistValue::Dictionary(d)) if d.is_empty()));
}

#[test]
fn test_parse_text_parameters_invalid_utf8() {
    let body = vec![0xFF, 0xFE, 0xFD]; // Invalid UTF-8
    let result = parse_text_parameters(&body);
    assert!(matches!(result, Err(BodyParseError::InvalidUtf8)));
}

#[test]
fn test_parse_text_parameters_malformed() {
    let body = b"key_without_value\nkey2: value2";
    let params = parse_text_parameters(body).unwrap();
    // Should skip malformed lines.
    assert!(!params.contains_key("key_without_value"));
    assert_eq!(params.get("key2"), Some(&"value2".to_string()));
}

#[test]
fn test_bplist_roundtrip() {
    let mut dict = HashMap::new();
    dict.insert("key".to_string(), PlistValue::Integer(42));
    let plist = PlistValue::Dictionary(dict);

    let encoded = encode_bplist_body(&plist).expect("Encode failed");
    // Ensure magic header
    assert_eq!(&encoded[..8], b"bplist00");

    let decoded = parse_bplist_body(&encoded).expect("Decode failed");
    assert_eq!(decoded.get_int("key"), Some(42));
}
