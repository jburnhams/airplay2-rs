use crate::protocol::plist::{DictBuilder, PlistValue};

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
        #[allow(clippy::approx_constant, reason = "Testing constant logic explicitly")]
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
