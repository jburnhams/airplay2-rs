use crate::protocol::plist::PlistValue;
use crate::receiver::ap2::body_handler::{
    PlistExt, PlistResponseBuilder, encode_text_parameters, parse_text_parameters,
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
