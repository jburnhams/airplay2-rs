use super::super::*;

#[test]
fn test_chacha_encrypt_decrypt() {
    let key = [0x42u8; 32];
    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

    let nonce = Nonce::from_counter(1);
    let plaintext = b"Hello, AirPlay!";

    let ciphertext = cipher.encrypt(&nonce, plaintext).unwrap();
    let decrypted = cipher.decrypt(&nonce, &ciphertext).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_chacha_ciphertext_is_larger() {
    let key = [0x42u8; 32];
    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

    let nonce = Nonce::from_counter(0);
    let plaintext = b"test";

    let ciphertext = cipher.encrypt(&nonce, plaintext).unwrap();

    // Ciphertext should be plaintext + 16 byte tag
    assert_eq!(ciphertext.len(), plaintext.len() + 16);
}

#[test]
fn test_chacha_decrypt_wrong_nonce_fails() {
    let key = [0x42u8; 32];
    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

    let nonce1 = Nonce::from_counter(1);
    let nonce2 = Nonce::from_counter(2);

    let ciphertext = cipher.encrypt(&nonce1, b"secret").unwrap();
    let result = cipher.decrypt(&nonce2, &ciphertext);

    assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
}

#[test]
fn test_chacha_encrypt_with_aad() {
    let key = [0x42u8; 32];
    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

    let nonce = Nonce::from_counter(1);
    let aad = b"header";
    let plaintext = b"body";

    let ciphertext = cipher.encrypt_with_aad(&nonce, aad, plaintext).unwrap();
    let decrypted = cipher.decrypt_with_aad(&nonce, aad, &ciphertext).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_chacha_decrypt_wrong_aad_fails() {
    let key = [0x42u8; 32];
    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

    let nonce = Nonce::from_counter(1);
    let ciphertext = cipher.encrypt_with_aad(&nonce, b"aad1", b"data").unwrap();

    let result = cipher.decrypt_with_aad(&nonce, b"aad2", &ciphertext);

    assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
}

#[test]
fn test_chacha_tamper() {
    let key = [0u8; 32];
    let nonce = Nonce::from_bytes(&[1u8; 12]).unwrap();
    let data = b"hello world";

    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();
    let mut encrypted = cipher.encrypt(&nonce, data).unwrap();

    // Tamper with data
    encrypted[0] ^= 0xFF;

    assert!(cipher.decrypt(&nonce, &encrypted).is_err());
}
