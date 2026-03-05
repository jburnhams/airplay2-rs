use std::collections::HashMap;

use crate::protocol::plist::PlistValue;

#[test]
fn test_encode_boolean() {
    let value = PlistValue::Boolean(true);
    let encoded = crate::protocol::plist::encode(&value).unwrap();
    assert_eq!(&encoded[0..8], b"bplist00");
}

#[test]
fn test_encode_integers() {
    for value in [
        0i64,
        1,
        127,
        128,
        255,
        256,
        65535,
        -1,
        -128,
        i64::MAX,
        i64::MIN,
    ] {
        let plist = PlistValue::Integer(value);
        let encoded = crate::protocol::plist::encode(&plist).unwrap();
        let decoded = crate::protocol::plist::decode(&encoded).expect("Decode failed");
        assert_eq!(decoded.as_i64(), Some(value), "Failed for value: {value}");
    }
}

#[test]
fn test_encode_string() {
    let value = PlistValue::String("hello world".to_string());
    let encoded = crate::protocol::plist::encode(&value).unwrap();
    let decoded = crate::protocol::plist::decode(&encoded).unwrap();
    assert_eq!(decoded.as_str(), Some("hello world"));
}

#[test]
fn test_encode_array() {
    let value = PlistValue::Array(vec![
        PlistValue::Integer(1),
        PlistValue::Integer(2),
        PlistValue::String("three".to_string()),
    ]);
    let encoded = crate::protocol::plist::encode(&value).unwrap();
    let decoded = crate::protocol::plist::decode(&encoded).unwrap();
    let arr = decoded.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_i64(), Some(1));
    assert_eq!(arr[2].as_str(), Some("three"));
}

#[test]
fn test_encode_dictionary() {
    let mut dict = HashMap::new();
    dict.insert("key1".to_string(), PlistValue::Integer(42));
    dict.insert("key2".to_string(), PlistValue::String("value".to_string()));

    let value = PlistValue::Dictionary(dict);
    let encoded = crate::protocol::plist::encode(&value).unwrap();
    let decoded = crate::protocol::plist::decode(&encoded).unwrap();

    let d = decoded.as_dict().unwrap();
    assert_eq!(d.get("key1").and_then(PlistValue::as_i64), Some(42));
    assert_eq!(d.get("key2").and_then(PlistValue::as_str), Some("value"));
}

#[test]
fn test_encode_decode_large_dict() {
    let mut dict = HashMap::new();
    for i in 0..100 {
        dict.insert(format!("key{i}"), PlistValue::Integer(i));
    }

    let value = PlistValue::Dictionary(dict);
    let encoded = crate::protocol::plist::encode(&value).unwrap();
    let decoded = crate::protocol::plist::decode(&encoded).unwrap();

    let d = decoded.as_dict().unwrap();
    assert_eq!(d.len(), 100);
    assert_eq!(d.get("key50").and_then(PlistValue::as_i64), Some(50));
}

#[test]
fn test_encode_decode_nested_mixed() {
    let mut dict = HashMap::new();
    dict.insert("int".to_string(), PlistValue::Integer(1));
    dict.insert(
        "arr".to_string(),
        PlistValue::Array(vec![
            PlistValue::Boolean(true),
            PlistValue::String("s".to_string()),
        ]),
    );

    let value = PlistValue::Dictionary(dict);
    let encoded = crate::protocol::plist::encode(&value).unwrap();
    let decoded = crate::protocol::plist::decode(&encoded).unwrap();

    let d = decoded.as_dict().unwrap();
    let arr = d.get("arr").unwrap().as_array().unwrap();
    assert_eq!(arr[0].as_bool(), Some(true));
}
