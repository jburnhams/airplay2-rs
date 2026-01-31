use super::super::*;

#[test]
fn test_hkdf_derive() {
    let ikm = b"input key material";
    let salt = b"salt";
    let info = b"info";

    let key = derive_key(Some(salt), ikm, info, 32).unwrap();

    assert_eq!(key.len(), 32);
}

#[test]
fn test_hkdf_deterministic() {
    let ikm = b"test";

    let key1 = derive_key(None, ikm, b"info", 32).unwrap();
    let key2 = derive_key(None, ikm, b"info", 32).unwrap();

    assert_eq!(key1, key2);
}

#[test]
fn test_hkdf_different_info() {
    let ikm = b"test";

    let key1 = derive_key(None, ikm, b"info1", 32).unwrap();
    let key2 = derive_key(None, ikm, b"info2", 32).unwrap();

    assert_ne!(key1, key2);
}

#[test]
fn test_airplay_keys() {
    let shared_secret = [0x42u8; 32];
    let salt = [0x00u8; 32];

    let keys = AirPlayKeys::derive(&shared_secret, &salt).unwrap();

    assert_eq!(keys.output_key.len(), 32);
    assert_eq!(keys.input_key.len(), 32);
    assert_ne!(keys.output_key, keys.input_key);
}
