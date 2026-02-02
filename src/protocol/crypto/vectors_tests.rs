use super::*;

#[test]
fn test_aes128ctr_key_length() {
    let key = [0u8; 15]; // Too short
    let iv = [0u8; 16];
    assert!(matches!(
        Aes128Ctr::new(&key, &iv),
        Err(CryptoError::InvalidKeyLength { expected: 16, .. })
    ));
}

#[test]
fn test_aes128ctr_iv_length() {
    let key = [0u8; 16];
    let iv = [0u8; 15]; // Too short
    assert!(matches!(
        Aes128Ctr::new(&key, &iv),
        Err(CryptoError::InvalidKeyLength { expected: 16, .. })
    ));
}

#[test]
fn test_aes128ctr_roundtrip() {
    let key = [1u8; 16];
    let iv = [2u8; 16];
    let plaintext = b"Hello World";

    let mut cipher = Aes128Ctr::new(&key, &iv).unwrap();
    let ciphertext = cipher.process(plaintext);

    // Should change data
    assert_ne!(ciphertext, plaintext);

    // Decrypt
    let mut decipher = Aes128Ctr::new(&key, &iv).unwrap();
    let decrypted = decipher.process(&ciphertext);

    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_aes128gcm_key_length() {
    let key = [0u8; 32]; // Too long (must be 16 for AES-128)
    assert!(matches!(
        Aes128Gcm::new(&key),
        Err(CryptoError::InvalidKeyLength { expected: 16, .. })
    ));
}

#[test]
fn test_aes128gcm_nonce_length() {
    let key = [0u8; 16];
    let cipher = Aes128Gcm::new(&key).unwrap();
    let nonce = [0u8; 11]; // Too short (must be 12)

    assert!(matches!(
        cipher.encrypt(&nonce, b"data"),
        Err(CryptoError::InvalidKeyLength { expected: 12, .. })
    ));
}

#[test]
fn test_aes128gcm_roundtrip() {
    let key = [3u8; 16];
    let nonce = [4u8; 12];
    let plaintext = b"Secret Data";

    let cipher = Aes128Gcm::new(&key).unwrap();
    let ciphertext = cipher.encrypt(&nonce, plaintext).unwrap();

    // GCM adds tag (16 bytes)
    assert_eq!(ciphertext.len(), plaintext.len() + 16);

    let decrypted = cipher.decrypt(&nonce, &ciphertext).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_chacha_key_length() {
    let key = [0u8; 16]; // Too short (must be 32)
    assert!(matches!(
        ChaCha20Poly1305Cipher::new(&key),
        Err(CryptoError::InvalidKeyLength { expected: 32, .. })
    ));
}

#[test]
fn test_chacha_nonce_length() {
    let nonce_bytes = [0u8; 11]; // Too short
    assert!(matches!(
        Nonce::from_bytes(&nonce_bytes),
        Err(CryptoError::InvalidKeyLength { expected: 12, .. })
    ));
}

#[test]
fn test_chacha_roundtrip() {
    let key = [5u8; 32];
    let nonce = Nonce::from_counter(1);
    let plaintext = b"ChaCha Data";
    let aad = b"header";

    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

    // Encrypt with AAD
    let ciphertext = cipher.encrypt_with_aad(&nonce, aad, plaintext).unwrap();

    // Tag is 16 bytes
    assert_eq!(ciphertext.len(), plaintext.len() + 16);

    // Decrypt
    let decrypted = cipher.decrypt_with_aad(&nonce, aad, &ciphertext).unwrap();
    assert_eq!(decrypted, plaintext);

    // Fail with wrong AAD
    assert!(
        cipher
            .decrypt_with_aad(&nonce, b"wrong", &ciphertext)
            .is_err()
    );
}
