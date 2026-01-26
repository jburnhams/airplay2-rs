use super::advertiser::*;

#[test]
fn test_format_mac_for_service() {
    let mac = [0x58, 0x55, 0xCA, 0x1A, 0xE2, 0x88];
    assert_eq!(format_mac_for_service(&mac), "5855CA1AE288");
}

#[test]
fn test_format_mac_with_zeros() {
    let mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
    assert_eq!(format_mac_for_service(&mac), "001122334455");
}

#[test]
fn test_parse_mac_string() {
    let mac = parse_mac_string("58:55:ca:1a:e2:88").unwrap();
    assert_eq!(mac, [0x58, 0x55, 0xca, 0x1a, 0xe2, 0x88]);
}

#[test]
fn test_parse_mac_string_invalid() {
    assert!(parse_mac_string("invalid").is_err());
    assert!(parse_mac_string("58:55:ca:1a:e2").is_err()); // Too short
    assert!(parse_mac_string("58:55:ca:1a:e2:88:99").is_err()); // Too long
    assert!(parse_mac_string("58:55:ZZ:1a:e2:88").is_err()); // Invalid hex
}

#[test]
fn test_stable_mac_generation() {
    // Should generate the same MAC for same input
    let mac1 = generate_stable_mac();
    let mac2 = generate_stable_mac();
    assert_eq!(mac1, mac2);

    // Should have locally-administered bit set
    assert!(
        mac1[0] & 0x02 != 0,
        "Locally-administered bit should be set"
    );
}

#[test]
fn test_status_flags_empty() {
    let flags = ReceiverStatusFlags::default();
    assert_eq!(flags.to_flags(), 0);
}

#[test]
fn test_status_flags_busy() {
    let flags = ReceiverStatusFlags {
        busy: true,
        ..Default::default()
    };
    assert_eq!(flags.to_flags(), 0x04);
}

#[test]
fn test_status_flags_combined() {
    let flags = ReceiverStatusFlags {
        problem: true,
        pin_required: true,
        busy: true,
        supports_legacy_pairing: true,
    };
    assert_eq!(flags.to_flags(), 0x0F);
}

#[test]
fn test_txt_record_builder_default() {
    let caps = RaopCapabilities::default();
    let status = ReceiverStatusFlags::default();
    let txt = TxtRecordBuilder::from_capabilities(&caps, &status);
    let records = txt.build_map();

    assert_eq!(records.get("txtvers"), Some(&"1".to_string()));
    assert_eq!(records.get("ch"), Some(&"2".to_string()));
    assert_eq!(records.get("sr"), Some(&"44100".to_string()));
    assert_eq!(records.get("ss"), Some(&"16".to_string()));
    assert_eq!(records.get("cn"), Some(&"0,1,2".to_string()));
    assert_eq!(records.get("et"), Some(&"0,1".to_string()));
    assert_eq!(records.get("tp"), Some(&"UDP".to_string()));
    assert_eq!(records.get("pw"), Some(&"false".to_string()));
}

#[test]
fn test_txt_record_password_required() {
    let caps = RaopCapabilities {
        password_required: true,
        ..Default::default()
    };
    let txt = TxtRecordBuilder::from_capabilities(&caps, &ReceiverStatusFlags::default());
    let records = txt.build_map();

    assert_eq!(records.get("pw"), Some(&"true".to_string()));
}

#[test]
fn test_txt_record_custom_codecs() {
    let caps = RaopCapabilities {
        codecs: vec![1], // ALAC only
        ..Default::default()
    };
    let txt = TxtRecordBuilder::from_capabilities(&caps, &ReceiverStatusFlags::default());
    let records = txt.build_map();

    assert_eq!(records.get("cn"), Some(&"1".to_string()));
}

#[test]
fn test_service_name_format() {
    let config = AdvertiserConfig {
        name: "Living Room".to_string(),
        mac_override: Some([0x5B, 0x55, 0xCA, 0x1A, 0xE2, 0x88]),
        ..Default::default()
    };
    let advertiser = RaopAdvertiser::new(config).unwrap();

    assert_eq!(advertiser.service_name(), "5B55CA1AE288@Living Room");
}

#[test]
fn test_service_name_special_characters() {
    let config = AdvertiserConfig {
        name: "John's Speaker".to_string(),
        mac_override: Some([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]),
        ..Default::default()
    };
    let advertiser = RaopAdvertiser::new(config).unwrap();

    assert_eq!(advertiser.service_name(), "AABBCCDDEEFF@John's Speaker");
}
