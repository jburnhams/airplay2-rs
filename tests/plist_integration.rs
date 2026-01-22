#[cfg(test)]
mod tests {
    use airplay2::protocol::plist;
    use airplay2::protocol::plist::PlistValue;
    use std::fs;
    use std::path::Path;

    fn load_fixture(name: &str) -> Vec<u8> {
        let path = Path::new("tests/fixtures").join(name);
        fs::read(&path).unwrap_or_else(|_| panic!("Failed to read fixture: {:?}", path))
    }

    #[test]
    fn test_integration_simple_dict() {
        let data = load_fixture("simple_dict.bplist");
        let decoded = plist::decode(&data).expect("Failed to decode simple_dict");
        let dict = decoded.as_dict().expect("Expected dictionary");

        assert_eq!(dict.get("key").and_then(PlistValue::as_str), Some("value"));
        assert_eq!(dict.get("int").and_then(PlistValue::as_i64), Some(42));
        assert_eq!(dict.get("bool").and_then(PlistValue::as_bool), Some(true));
    }

    #[test]
    fn test_integration_nested_dict() {
        let data = load_fixture("nested_dict.bplist");
        let decoded = plist::decode(&data).expect("Failed to decode nested_dict");
        let dict = decoded.as_dict().expect("Expected dictionary");

        let parent = dict.get("parent").and_then(PlistValue::as_dict).unwrap();
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
    fn test_integration_array() {
        let data = load_fixture("array.bplist");
        let decoded = plist::decode(&data).expect("Failed to decode array");
        let arr = decoded.as_array().expect("Expected array");

        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_i64(), Some(1));
        assert_eq!(arr[1].as_str(), Some("two"));
        assert_eq!(arr[2].as_bool(), Some(false));
    }

    #[test]
    fn test_integration_large_dict() {
        let data = load_fixture("large_dict.bplist");
        let decoded = plist::decode(&data).expect("Failed to decode large_dict");
        let dict = decoded.as_dict().expect("Expected dictionary");

        assert_eq!(dict.len(), 100);
        assert_eq!(dict.get("key_50").and_then(PlistValue::as_i64), Some(50));
        assert_eq!(dict.get("key_99").and_then(PlistValue::as_i64), Some(99));
    }

    #[test]
    fn test_integration_types() {
        let data = load_fixture("types.bplist");
        let decoded = plist::decode(&data).expect("Failed to decode types");
        let dict = decoded.as_dict().expect("Expected dictionary");

        let data_val = dict.get("data").and_then(PlistValue::as_bytes).unwrap();
        assert_eq!(data_val, &[0xCA, 0xFE, 0xBA, 0xBE]);

        let date_val = dict.get("date").and_then(PlistValue::as_date).unwrap();
        assert!(date_val.abs() < f64::EPSILON); // 0.0

        let real_val = dict.get("real").and_then(PlistValue::as_f64).unwrap();
        #[allow(clippy::approx_constant)]
        {
            assert!((real_val - 3.14159).abs() < 1e-5);
        }
    }
}
