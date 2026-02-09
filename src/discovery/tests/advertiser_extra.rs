use crate::discovery::advertiser::*;

#[test]
fn test_txt_record_boolean_values() {
    let mut builder = TxtRecordBuilder::new();
    builder.add("bool_true", "true");
    builder.add("bool_false", "false");

    let records = builder.build_map();
    assert_eq!(records.get("bool_true"), Some(&"true".to_string()));
    assert_eq!(records.get("bool_false"), Some(&"false".to_string()));
}

#[test]
fn test_txt_record_list_formatting() {
    let caps = RaopCapabilities {
        codecs: vec![0, 1, 2, 3],
        encryption_types: vec![],
        metadata_types: vec![1],
        ..Default::default()
    };
    let txt = TxtRecordBuilder::from_capabilities(&caps, &ReceiverStatusFlags::default());
    let records = txt.build_map();

    assert_eq!(records.get("cn"), Some(&"0,1,2,3".to_string()));
    assert_eq!(records.get("et"), Some(&"".to_string()));
    assert_eq!(records.get("md"), Some(&"1".to_string()));
}

#[test]
fn test_txt_record_hex_formatting() {
    let flags = ReceiverStatusFlags {
        problem: true,
        ..Default::default()
    };
    // 0x01
    let txt = TxtRecordBuilder::from_capabilities(&RaopCapabilities::default(), &flags);
    let records = txt.build_map();
    assert_eq!(records.get("sf"), Some(&"0x1".to_string()));

    let flags_all = ReceiverStatusFlags {
        problem: true,
        pin_required: true,
        busy: true,
        supports_legacy_pairing: true,
    };
    // 0x0F
    let txt_all = TxtRecordBuilder::from_capabilities(&RaopCapabilities::default(), &flags_all);
    let records_all = txt_all.build_map();
    assert_eq!(records_all.get("sf"), Some(&"0xf".to_string()));
}

#[test]
fn test_txt_record_all_keys_present() {
    let caps = RaopCapabilities::default();
    let status = ReceiverStatusFlags::default();
    let txt = TxtRecordBuilder::from_capabilities(&caps, &status);
    let records = txt.build_map();

    let expected_keys = [
        "txtvers", "ch", "sr", "ss", "cn", "et", "md", "tp", "pw", "am", "vn", "vs", "sf", "ft",
    ];

    for key in expected_keys {
        assert!(records.contains_key(key), "Missing key: {}", key);
    }
}

#[test]
fn test_advertiser_config_custom_port() {
    let config = AdvertiserConfig {
        port: 1234,
        ..Default::default()
    };
    assert_eq!(config.port, 1234);
}

#[test]
fn test_advertiser_config_custom_name() {
    let config = AdvertiserConfig {
        name: "Custom Name".to_string(),
        ..Default::default()
    };
    assert_eq!(config.name, "Custom Name");
}
