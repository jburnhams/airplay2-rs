use crate::protocol::plist::{PlistDecodeError, PlistValue};
use std::collections::HashMap;

#[test]
fn test_decode_invalid_magic() {
    let data = b"notplist";
    let result = crate::protocol::plist::decode(data);

    assert!(matches!(result, Err(PlistDecodeError::InvalidMagic(_))));
}

#[test]
fn test_decode_too_small() {
    let data = b"short";
    let result = crate::protocol::plist::decode(data);

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

    let res = crate::protocol::plist::decode(&data);
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
        crate::protocol::plist::decode(&data),
        Err(PlistDecodeError::InvalidObjectMarker(0xFF))
    ));
}

#[test]
fn test_decode_boolean() {
    let val = PlistValue::Boolean(true);
    let bytes = crate::protocol::plist::encode(&val).unwrap();
    let decoded = crate::protocol::plist::decode(&bytes).unwrap();
    assert!(matches!(decoded, PlistValue::Boolean(true)));

    let val = PlistValue::Boolean(false);
    let bytes = crate::protocol::plist::encode(&val).unwrap();
    let decoded = crate::protocol::plist::decode(&bytes).unwrap();
    assert!(matches!(decoded, PlistValue::Boolean(false)));
}

#[test]
fn test_decode_empty_dict() {
    let val = PlistValue::Dictionary(HashMap::new());
    let bytes = crate::protocol::plist::encode(&val).unwrap();
    let decoded = crate::protocol::plist::decode(&bytes).unwrap();
    match decoded {
        PlistValue::Dictionary(d) => assert!(d.is_empty()),
        _ => panic!("Expected dictionary"),
    }
}

#[test]
fn test_decode_integers() {
    for &i in &[0, 42, 127, 255, 65535, 100_000, -1, -100] {
        let val = PlistValue::Integer(i);
        let bytes = crate::protocol::plist::encode(&val).unwrap();
        let decoded = crate::protocol::plist::decode(&bytes).unwrap();
        match decoded {
            PlistValue::Integer(v) => assert_eq!(v, i),
            _ => panic!("Expected integer"),
        }
    }
}

#[test]
fn test_decode_string_ascii() {
    let s = "Hello World";
    let val = PlistValue::String(s.to_string());
    let bytes = crate::protocol::plist::encode(&val).unwrap();
    let decoded = crate::protocol::plist::decode(&bytes).unwrap();
    match decoded {
        PlistValue::String(v) => assert_eq!(v, s),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_decode_string_unicode() {
    let s = "Hello 🌍";
    let val = PlistValue::String(s.to_string());
    let bytes = crate::protocol::plist::encode(&val).unwrap();
    let decoded = crate::protocol::plist::decode(&bytes).unwrap();
    match decoded {
        PlistValue::String(v) => assert_eq!(v, s),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_decode_array() {
    let val = PlistValue::Array(vec![
        PlistValue::Integer(1),
        PlistValue::String("two".to_string()),
    ]);
    let bytes = crate::protocol::plist::encode(&val).unwrap();
    let decoded = crate::protocol::plist::decode(&bytes).unwrap();
    match decoded {
        PlistValue::Array(arr) => {
            assert_eq!(arr.len(), 2);
            assert!(matches!(arr[0], PlistValue::Integer(1)));
            assert!(matches!(arr[1], PlistValue::String(ref s) if s == "two"));
        }
        _ => panic!("Expected array"),
    }
}

#[test]
fn test_decode_nested_dict() {
    let mut inner = HashMap::new();
    inner.insert("a".to_string(), PlistValue::Integer(1));
    let mut outer = HashMap::new();
    outer.insert("inner".to_string(), PlistValue::Dictionary(inner));

    let val = PlistValue::Dictionary(outer);
    let bytes = crate::protocol::plist::encode(&val).unwrap();
    let decoded = crate::protocol::plist::decode(&bytes).unwrap();

    // Validation logic
    if let PlistValue::Dictionary(d) = decoded {
        if let Some(PlistValue::Dictionary(inner_d)) = d.get("inner") {
            assert_eq!(inner_d.get("a").and_then(|v| v.as_i64()), Some(1));
        } else {
            panic!("Nested dictionary missing");
        }
    } else {
        panic!("Expected dictionary");
    }
}

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
        crate::protocol::plist::decode(&data),
        Err(PlistDecodeError::CircularReference)
    ));
}

#[test]
fn test_decode_empty_string() {
    let value = PlistValue::String("".to_string());
    let encoded = crate::protocol::plist::encode(&value).unwrap();
    let decoded = crate::protocol::plist::decode(&encoded).unwrap();
    assert_eq!(decoded.as_str(), Some(""));
}

#[test]
fn test_decode_deeply_nested_recursion_limit() {
    let mut val = PlistValue::Integer(0);
    for _ in 0..500 {
        let mut map = HashMap::new();
        map.insert("n".to_string(), val);
        val = PlistValue::Dictionary(map);
    }

    let encoded = crate::protocol::plist::encode(&val).unwrap();
    let decoded = crate::protocol::plist::decode(&encoded).unwrap();

    // Verify depth?
    let mut curr = &decoded;
    for _ in 0..500 {
        if let PlistValue::Dictionary(d) = curr {
            curr = d.get("n").unwrap();
        } else {
            panic!("Expected dictionary");
        }
    }
    assert!(matches!(curr, PlistValue::Integer(0)));
}

#[test]
fn test_decode_integer_overflow_edge_cases() {
    // Test u64 max.
    let val = PlistValue::UnsignedInteger(u64::MAX);
    let encoded = crate::protocol::plist::encode(&val).unwrap();
    let decoded = crate::protocol::plist::decode(&encoded).unwrap();

    assert_eq!(decoded, val);
}

#[test]
fn test_decode_invalid_utf8() {
    // 0x5 string with invalid utf8
    let mut data = b"bplist00".to_vec();
    // Object: ASCII string of length 1
    // 0x51
    data.push(0x51);
    // Invalid byte 0xFF
    data.push(0xFF);

    // Construct trailer...
    let offset_table_start = data.len();
    data.push(8); // Offset of object 0 is 8

    // Trailer
    data.extend_from_slice(&[0; 5]); // unused
    data.push(0); // sort
    data.push(1); // offset_size
    data.push(1); // ref_size
    data.extend_from_slice(&1u64.to_be_bytes()); // num_objects
    data.extend_from_slice(&0u64.to_be_bytes()); // root
    data.extend_from_slice(&(offset_table_start as u64).to_be_bytes()); // offset table

    let result = crate::protocol::plist::decode(&data);
    assert!(matches!(result, Err(PlistDecodeError::InvalidUtf8)));
}

#[test]
fn test_decode_unknown_marker() {
    let mut data = b"bplist00".to_vec();
    data.push(0x70); // 0x7 is not a standard marker type in our implementation

    let offset_table_start = data.len();
    data.push(8); // Offset of object 0 is 8

    // Trailer
    data.extend_from_slice(&[0; 5]);
    data.push(0);
    data.push(1);
    data.push(1);
    data.extend_from_slice(&1u64.to_be_bytes());
    data.extend_from_slice(&0u64.to_be_bytes());
    data.extend_from_slice(&(offset_table_start as u64).to_be_bytes());

    let result = crate::protocol::plist::decode(&data);
    assert!(matches!(
        result,
        Err(PlistDecodeError::InvalidObjectMarker(0x70))
    ));
}
