use crate::protocol::raop::encryption::*;

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let key = [0x42u8; AES_KEY_SIZE];
    let iv = [0x00u8; AES_IV_SIZE];

    let encryptor = RaopEncryptor::new(key, iv);
    let decryptor = RaopDecryptor::new(key, iv);

    let original = vec![0xAA; FRAME_SIZE];
    let packet_index = 0;

    let encrypted = encryptor.encrypt(&original, packet_index).unwrap();
    assert_ne!(encrypted, original);

    let decrypted = decryptor.decrypt(&encrypted, packet_index).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_cbc_matches_nist_vector_and_leaves_tail_clear() {
    // NIST SP 800-38A F.2.1 (AES-128-CBC), two blocks, plus 5 trailing bytes
    // that RAOP sends unencrypted.
    let key: [u8; 16] = hex("2b7e151628aed2a6abf7158809cf4f3c").try_into().unwrap();
    let iv: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
    let mut plain = hex("6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e51");
    plain.extend_from_slice(b"tail!");

    let encryptor = RaopEncryptor::new(key, iv);
    let encrypted = encryptor.encrypt(&plain, 0).unwrap();
    assert_eq!(
        encrypted[..32],
        hex("7649abac8119b246cee98e9b12e9197d5086cb9b507219ee95db113a917678b2")[..]
    );
    assert_eq!(&encrypted[32..], b"tail!");

    // Every packet restarts from the session IV.
    assert_eq!(encryptor.encrypt(&plain, 7).unwrap(), encrypted);
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn test_disabled_encryption() {
    let encryptor = RaopEncryptor::disabled();

    let data = vec![0xAA; 100];
    let encrypted = encryptor.encrypt(&data, 0).unwrap();

    // Should be unchanged
    assert_eq!(encrypted, data);
}

#[test]
fn test_encrypt_in_place() {
    let key = [0x42u8; AES_KEY_SIZE];
    let iv = [0x00u8; AES_IV_SIZE];

    let encryptor = RaopEncryptor::new(key, iv);
    let decryptor = RaopDecryptor::new(key, iv);

    let original = vec![0xAA; FRAME_SIZE];
    let mut data = original.clone();

    encryptor.encrypt_in_place(&mut data, 0).unwrap();
    assert_ne!(data, original);

    let decrypted = decryptor.decrypt(&data, 0).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_encryption_mode_parsing() {
    assert_eq!(EncryptionMode::from_txt(0), Some(EncryptionMode::None));
    assert_eq!(EncryptionMode::from_txt(1), Some(EncryptionMode::Rsa));
    assert_eq!(EncryptionMode::from_txt(3), Some(EncryptionMode::FairPlay));
    assert_eq!(EncryptionMode::from_txt(99), None);

    assert!(EncryptionMode::None.is_supported());
    assert!(EncryptionMode::Rsa.is_supported());
    assert!(!EncryptionMode::FairPlay.is_supported());
}

#[test]
fn test_sequential_packet_encryption() {
    let key = [0x42u8; AES_KEY_SIZE];
    let iv = [0x00u8; AES_IV_SIZE];

    let encryptor = RaopEncryptor::new(key, iv);
    let decryptor = RaopDecryptor::new(key, iv);

    // Simulate streaming multiple packets
    for i in 0..10u64 {
        let data = vec![(i & 0xFF) as u8; FRAME_SIZE];
        let encrypted = encryptor.encrypt(&data, i).unwrap();
        let decrypted = decryptor.decrypt(&encrypted, i).unwrap();
        assert_eq!(decrypted, data);
    }
}
