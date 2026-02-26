use std::collections::HashMap;

use crate::types::raop::*;

#[test]
fn test_parse_capabilities_basic() {
    let mut records = HashMap::new();
    records.insert("ch".to_string(), "2".to_string());
    records.insert("cn".to_string(), "0,1,2".to_string());
    records.insert("et".to_string(), "0,1".to_string());
    records.insert("sr".to_string(), "44100".to_string());
    records.insert("ss".to_string(), "16".to_string());

    let caps = RaopCapabilities::from_txt_records(&records);

    assert_eq!(caps.channels, 2);
    assert_eq!(caps.sample_rate, 44100);
    assert_eq!(caps.sample_size, 16);
    assert!(caps.supports_codec(RaopCodec::Pcm));
    assert!(caps.supports_codec(RaopCodec::Alac));
    assert!(caps.supports_codec(RaopCodec::Aac));
    assert!(caps.supports_rsa());
    assert!(caps.supports_unencrypted());
}

#[test]
fn test_parse_capabilities_airport_express() {
    // Typical AirPort Express TXT records
    let mut records = HashMap::new();
    records.insert("txtvers".to_string(), "1".to_string());
    records.insert("ch".to_string(), "2".to_string());
    records.insert("cn".to_string(), "0,1,2,3".to_string());
    records.insert("da".to_string(), "true".to_string());
    records.insert("et".to_string(), "0,3,5".to_string());
    records.insert("md".to_string(), "0,1,2".to_string());
    records.insert("pw".to_string(), "false".to_string());
    records.insert("sr".to_string(), "44100".to_string());
    records.insert("ss".to_string(), "16".to_string());
    records.insert("tp".to_string(), "UDP".to_string());
    records.insert("vs".to_string(), "130.14".to_string());
    records.insert("am".to_string(), "AirPort10,115".to_string());

    let caps = RaopCapabilities::from_txt_records(&records);

    assert!(caps.metadata_support);
    assert!(!caps.password_required);
    assert_eq!(caps.model, Some("AirPort10,115".to_string()));
    assert_eq!(caps.preferred_codec(), Some(RaopCodec::Alac));
}

#[test]
fn test_preferred_encryption_rsa() {
    let mut records = HashMap::new();
    records.insert("et".to_string(), "0,1".to_string());

    let caps = RaopCapabilities::from_txt_records(&records);

    assert_eq!(caps.preferred_encryption(), Some(RaopEncryption::Rsa));
}

#[test]
fn test_preferred_encryption_none_only() {
    let mut records = HashMap::new();
    records.insert("et".to_string(), "0".to_string());

    let caps = RaopCapabilities::from_txt_records(&records);

    assert_eq!(caps.preferred_encryption(), Some(RaopEncryption::None));
}

#[test]
fn test_preferred_encryption_fairplay_unsupported() {
    let mut records = HashMap::new();
    records.insert("et".to_string(), "3".to_string()); // FairPlay only

    let caps = RaopCapabilities::from_txt_records(&records);

    assert_eq!(caps.preferred_encryption(), None);
}

#[test]
fn test_codec_preference() {
    let mut records = HashMap::new();
    records.insert("cn".to_string(), "0,2".to_string()); // PCM and AAC

    let caps = RaopCapabilities::from_txt_records(&records);

    // Should prefer AAC over PCM
    assert_eq!(caps.preferred_codec(), Some(RaopCodec::Aac));
}

#[test]
fn test_empty_records() {
    let records = HashMap::new();
    let caps = RaopCapabilities::from_txt_records(&records);

    // Should use sensible defaults
    assert_eq!(caps.channels, 2);
    assert_eq!(caps.sample_rate, 44100);
    assert_eq!(caps.sample_size, 16);
    assert_eq!(caps.transport, "UDP");
}
