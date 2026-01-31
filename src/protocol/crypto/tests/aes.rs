use super::super::*;

#[test]
fn test_aes_ctr_encrypt_decrypt() {
    let key = [0x42u8; 16];
    let iv = [0x00u8; 16];

    let mut cipher1 = Aes128Ctr::new(&key, &iv).unwrap();
    let mut cipher2 = Aes128Ctr::new(&key, &iv).unwrap();

    let plaintext = b"Hello, AirPlay audio!";
    let ciphertext = cipher1.process(plaintext);

    assert_ne!(&ciphertext, plaintext);

    let decrypted = cipher2.process(&ciphertext);
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_aes_ctr_in_place() {
    let key = [0x42u8; 16];
    let iv = [0x00u8; 16];

    let mut cipher = Aes128Ctr::new(&key, &iv).unwrap();

    let mut data = b"test data".to_vec();
    let original = data.clone();

    cipher.apply_keystream(&mut data);
    assert_ne!(data, original);

    // Reset cipher and decrypt
    let mut cipher = Aes128Ctr::new(&key, &iv).unwrap();
    cipher.apply_keystream(&mut data);
    assert_eq!(data, original);
}

#[test]
fn test_aes_gcm_encrypt_decrypt() {
    let key = [0x42u8; 16];
    let nonce = [0x00u8; 12];

    let cipher = Aes128Gcm::new(&key).unwrap();

    let plaintext = b"Secret audio data";
    let ciphertext = cipher.encrypt(&nonce, plaintext).unwrap();
    let decrypted = cipher.decrypt(&nonce, &ciphertext).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_aes_gcm_tamper_detection() {
    let key = [0x42u8; 16];
    let nonce = [0x00u8; 12];

    let cipher = Aes128Gcm::new(&key).unwrap();

    let mut ciphertext = cipher.encrypt(&nonce, b"data").unwrap();
    ciphertext[0] ^= 0xFF; // Tamper with ciphertext

    let result = cipher.decrypt(&nonce, &ciphertext);
    assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
}

#[test]
fn test_aes_ctr_seek() {
    let key = [0u8; 16];
    let iv = [0u8; 16];
    let data = b"hello world";

    let mut cipher1 = Aes128Ctr::new(&key, &iv).unwrap();
    let full_ciphertext = cipher1.process(data);

    // Decrypt only last 5 bytes
    let mut cipher2 = Aes128Ctr::new(&key, &iv).unwrap();
    let offset = (data.len() - 5) as u64;
    cipher2.seek(offset);

    let mut partial = full_ciphertext[full_ciphertext.len() - 5..].to_vec();
    cipher2.apply_keystream(&mut partial);

    assert_eq!(partial, &data[data.len() - 5..]);
}
