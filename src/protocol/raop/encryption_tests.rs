use super::encryption::*;

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
fn test_different_packets_different_ciphertext() {
    let key = [0x42u8; AES_KEY_SIZE];
    let iv = [0x00u8; AES_IV_SIZE];

    let encryptor = RaopEncryptor::new(key, iv);

    let data = vec![0xAA; FRAME_SIZE];

    let encrypted1 = encryptor.encrypt(&data, 0).unwrap();
    let encrypted2 = encryptor.encrypt(&data, 1).unwrap();

    // Same plaintext, different packet index -> different ciphertext
    assert_ne!(encrypted1, encrypted2);
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
