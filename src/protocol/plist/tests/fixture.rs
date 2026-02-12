use crate::protocol::plist::{PlistValue, decode};

#[test]
fn test_fixture_simple_dict() {
    let data = std::fs::read("tests/fixtures/simple_dict.bplist").expect("Fixture not found");
    let decoded = decode(&data).unwrap();
    let d = decoded.as_dict().unwrap();
    assert_eq!(d.get("key").and_then(PlistValue::as_str), Some("value"));
    assert_eq!(d.get("int").and_then(PlistValue::as_i64), Some(42));
    assert_eq!(d.get("bool").and_then(PlistValue::as_bool), Some(true));
}

#[test]
fn test_fixture_nested_dict() {
    let data = std::fs::read("tests/fixtures/nested_dict.bplist").expect("Fixture not found");
    let decoded = decode(&data).unwrap();
    let d = decoded.as_dict().unwrap();
    let parent = d.get("parent").unwrap().as_dict().unwrap();
    assert_eq!(
        parent.get("child").and_then(PlistValue::as_str),
        Some("hello")
    );
    assert_eq!(
        parent.get("grandchild").and_then(PlistValue::as_i64),
        Some(123)
    );
}

#[test]
fn test_fixture_array() {
    let data = std::fs::read("tests/fixtures/array.bplist").expect("Fixture not found");
    let decoded = decode(&data).unwrap();
    let arr = decoded.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_i64(), Some(1));
    assert_eq!(arr[1].as_str(), Some("two"));
    assert_eq!(arr[2].as_bool(), Some(false));
}

#[test]
fn test_fixture_types() {
    let data = std::fs::read("tests/fixtures/types.bplist").expect("Fixture not found");
    let decoded = decode(&data).unwrap();
    let d = decoded.as_dict().unwrap();

    // Data
    let data_val = d.get("data").unwrap().as_bytes().unwrap();
    assert_eq!(data_val, &[0xCA, 0xFE, 0xBA, 0xBE]);

    // Real
    let real_val = d.get("real").unwrap().as_f64().unwrap();
    assert!((real_val - 3.14159).abs() < 1e-5);
}
