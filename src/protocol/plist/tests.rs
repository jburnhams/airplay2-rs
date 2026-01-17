use super::*;
use std::collections::HashMap;

// --- Tests from mod.rs ---

#[test]
fn test_plist_value_accessors() {
    let value = PlistValue::Integer(42);
    assert_eq!(value.as_i64(), Some(42));
    assert_eq!(value.as_str(), None);
    assert_eq!(value.as_bool(), None);
}

#[test]
fn test_plist_value_from_conversions() {
    assert!(matches!(PlistValue::from(true), PlistValue::Boolean(true)));
    assert!(matches!(PlistValue::from(42i64), PlistValue::Integer(42)));
    // Approximate float comparison
    match PlistValue::from(std::f64::consts::PI) {
        #[allow(clippy::approx_constant)]
        PlistValue::Real(f) => assert!((f - std::f64::consts::PI).abs() < f64::EPSILON),
        _ => panic!("Expected Real"),
    }

    match PlistValue::from("hello") {
        PlistValue::String(s) => assert_eq!(s, "hello"),
        _ => panic!("Expected String"),
    }
}

#[test]
fn test_dict_builder() {
    let dict = DictBuilder::new()
        .insert("key1", "value1")
        .insert("key2", 42i64)
        .insert_opt("key3", Some("present"))
        .insert_opt::<String>("key4", None)
        .build();

    let d = dict.as_dict().unwrap();
    assert_eq!(d.len(), 3);
    assert!(d.contains_key("key1"));
    assert!(d.contains_key("key2"));
    assert!(d.contains_key("key3"));
    assert!(!d.contains_key("key4"));
}

#[test]
fn test_plist_dict_macro() {
    let dict = plist_dict! {
        "name" => "test",
        "count" => 5i64,
    };

    let d = dict.as_dict().unwrap();
    assert_eq!(d.get("name").and_then(PlistValue::as_str), Some("test"));
    assert_eq!(d.get("count").and_then(PlistValue::as_i64), Some(5));
}

// --- Tests from decode.rs ---

#[test]
fn test_decode_invalid_magic() {
    let data = b"notplist";
    let result = super::decode(data);

    assert!(matches!(result, Err(PlistDecodeError::InvalidMagic(_))));
}

#[test]
fn test_decode_too_small() {
    let data = b"short";
    let result = super::decode(data);

    assert!(matches!(
        result,
        Err(PlistDecodeError::BufferTooSmall { .. })
    ));
}

#[test]
fn test_decode_invalid_trailer_offset() {
    // Trailer points to offset table outside file
    let mut data = b"bplist00".to_vec();
    data.extend_from_slice(&[0; 32]); // Filler

    // Overwrite trailer manually
    let len = data.len();
    // offset_table_offset at the end (last 8 bytes of file)
    let bad_offset = 9999u64;
    let offset_bytes = bad_offset.to_be_bytes();
    for i in 0..8 {
        data[len - 8 + i] = offset_bytes[i];
    }

    let res = super::decode(&data);
    // It might be BufferTooSmall or InvalidTrailer depending on check order
    assert!(matches!(
        res,
        Err(PlistDecodeError::BufferTooSmall { .. } | PlistDecodeError::InvalidTrailer)
    ));
}

#[test]
fn test_decode_invalid_object_marker() {
    let mut data = b"bplist00".to_vec();
    data.push(0xFF); // Invalid marker at offset 8

    let offset_table_start = data.len();
    data.push(8); // Offset of object (index 0) is 8

    // Trailer
    data.extend_from_slice(&[0; 5]);
    data.push(0); // sort
    data.push(1); // offset_size
    data.push(1); // object_ref_size
    data.extend_from_slice(&1u64.to_be_bytes()); // num_objects
    data.extend_from_slice(&0u64.to_be_bytes()); // root_index
    data.extend_from_slice(&(offset_table_start as u64).to_be_bytes());

    assert!(matches!(
        super::decode(&data),
        Err(PlistDecodeError::InvalidObjectMarker(0xFF))
    ));
}

// --- Tests from encode.rs ---

#[test]
fn test_encode_boolean() {
    let value = PlistValue::Boolean(true);
    let encoded = super::encode(&value).unwrap();
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
        let encoded = super::encode(&plist).unwrap();
        let decoded = super::decode(&encoded).expect("Decode failed");
        assert_eq!(decoded.as_i64(), Some(value), "Failed for value: {value}");
    }
}

#[test]
fn test_encode_string() {
    let value = PlistValue::String("hello world".to_string());
    let encoded = super::encode(&value).unwrap();
    let decoded = super::decode(&encoded).unwrap();
    assert_eq!(decoded.as_str(), Some("hello world"));
}

#[test]
fn test_encode_array() {
    let value = PlistValue::Array(vec![
        PlistValue::Integer(1),
        PlistValue::Integer(2),
        PlistValue::String("three".to_string()),
    ]);
    let encoded = super::encode(&value).unwrap();
    let decoded = super::decode(&encoded).unwrap();
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
    let encoded = super::encode(&value).unwrap();
    let decoded = super::decode(&encoded).unwrap();

    let d = decoded.as_dict().unwrap();
    assert_eq!(d.get("key1").and_then(PlistValue::as_i64), Some(42));
    assert_eq!(d.get("key2").and_then(PlistValue::as_str), Some("value"));
}

// --- Tests from airplay.rs ---

#[test]
fn test_track_info_to_plist() {
    use crate::protocol::plist::airplay::track_info_to_plist;
    use crate::types::TrackInfo;

    let track = TrackInfo::new("http://url", "Title", "Artist")
        .with_album("Album")
        .with_duration(123.0);

    let plist = track_info_to_plist(&track);
    let dict = plist.as_dict().unwrap();

    assert_eq!(
        dict.get("title").and_then(PlistValue::as_str),
        Some("Title")
    );
    assert_eq!(
        dict.get("duration").and_then(PlistValue::as_f64),
        Some(123.0)
    );
}

// --- Extra Tests ---

#[test]
fn test_decode_circular_reference() {
    // Manually construct a plist with a circular reference
    // Root -> Array -> Root
    let mut data = b"bplist00".to_vec();

    // Object 0: Array [Object 0]
    // 0xA1 means Array with 1 element
    data.push(0xA1);
    // Reference to Object 0 (index 0)
    // Ref size 1, index 0 -> 0x00
    data.push(0x00);

    // Offset table
    // Offset of object 0 is 8
    let offset_table_start = data.len();
    data.push(8);

    // Trailer
    data.extend_from_slice(&[0; 5]);
    data.push(0); // sort
    data.push(1); // offset_size
    data.push(1); // object_ref_size
    data.extend_from_slice(&1u64.to_be_bytes()); // num_objects
    data.extend_from_slice(&0u64.to_be_bytes()); // root_index
    data.extend_from_slice(&(offset_table_start as u64).to_be_bytes());

    assert!(matches!(
        super::decode(&data),
        Err(PlistDecodeError::CircularReference)
    ));
}

#[test]
fn test_encode_decode_large_dict() {
    let mut dict = HashMap::new();
    for i in 0..100 {
        dict.insert(format!("key{i}"), PlistValue::Integer(i));
    }

    let value = PlistValue::Dictionary(dict);
    let encoded = super::encode(&value).unwrap();
    let decoded = super::decode(&encoded).unwrap();

    let d = decoded.as_dict().unwrap();
    assert_eq!(d.len(), 100);
    assert_eq!(d.get("key50").and_then(PlistValue::as_i64), Some(50));
}

#[test]
fn test_decode_empty_string() {
    let value = PlistValue::String("".to_string());
    let encoded = super::encode(&value).unwrap();
    let decoded = super::decode(&encoded).unwrap();
    assert_eq!(decoded.as_str(), Some(""));
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
    let encoded = super::encode(&value).unwrap();
    let decoded = super::decode(&encoded).unwrap();

    let d = decoded.as_dict().unwrap();
    let arr = d.get("arr").unwrap().as_array().unwrap();
    assert_eq!(arr[0].as_bool(), Some(true));
}
