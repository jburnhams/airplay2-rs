use base64::Engine;

use crate::receiver::ap2::advertisement::{Ap2TxtRecord, txt_keys};
use crate::receiver::ap2::config::Ap2Config;

#[test]
fn test_txt_record_contains_required_fields() {
    let config = Ap2Config::new("Test Speaker");
    let public_key = [0x42u8; 32];
    let txt = Ap2TxtRecord::from_config(&config, &public_key);

    // Required fields
    assert!(txt.get(txt_keys::DEVICE_ID).is_some());
    assert!(txt.get(txt_keys::FEATURES).is_some());
    assert!(txt.get(txt_keys::STATUS_FLAGS).is_some());
    assert!(txt.get(txt_keys::PUBLIC_KEY).is_some());
    assert!(txt.get(txt_keys::PAIRING_IDENTITY).is_some());
    assert!(txt.get(txt_keys::MODEL).is_some());
}

#[test]
fn test_feature_flags_in_txt() {
    let mut config = Ap2Config::new("Test Speaker");
    config.multi_room_enabled = true;

    let txt = Ap2TxtRecord::from_config(&config, &[0u8; 32]);
    let features = txt.get(txt_keys::FEATURES).unwrap();

    // Should have two hex values
    assert!(features.contains(','));
    assert!(features.starts_with("0x") || features.starts_with("0X"));
}

#[test]
fn test_password_flag_in_status() {
    let mut config = Ap2Config::new("Test Speaker");
    config.password = Some("secret".to_string());

    let txt = Ap2TxtRecord::from_config(&config, &[0u8; 32]);
    let flags_str = txt.get(txt_keys::STATUS_FLAGS).unwrap();

    // Parse flags and check password bit (bit 4)
    let flags = u32::from_str_radix(flags_str.trim_start_matches("0x"), 16).unwrap();
    assert!((flags & (1 << 4)) != 0, "Password flag should be set");
}

#[test]
fn test_public_key_base64_encoded() {
    let config = Ap2Config::new("Test Speaker");
    let public_key = [0xAB; 32];
    let txt = Ap2TxtRecord::from_config(&config, &public_key);

    let pk_b64 = txt.get(txt_keys::PUBLIC_KEY).unwrap();

    // Verify it's valid base64 and decodes to 32 bytes
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(pk_b64)
        .expect("Should be valid base64");

    assert_eq!(decoded.len(), 32);
    assert_eq!(decoded, public_key.to_vec());
}
