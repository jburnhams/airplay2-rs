use crate::receiver::ap2::encrypted_channel::{EncryptedChannel, EncryptionError};

fn create_test_channel() -> (EncryptedChannel, EncryptedChannel) {
    // Create two channels with swapped keys (simulating sender/receiver)
    let key_a = [0x41u8; 32];
    let key_b = [0x42u8; 32];

    let sender = EncryptedChannel::new(key_a, key_b);
    let receiver = EncryptedChannel::new(key_b, key_a);

    (sender, receiver)
}

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let (mut sender, mut receiver) = create_test_channel();

    let message = b"Hello, AirPlay 2!";

    // Encrypt on sender side
    let encrypted = sender.encrypt(message).unwrap();

    // Decrypt on receiver side
    receiver.feed(&encrypted);
    let decrypted = receiver.decrypt().unwrap().unwrap();

    assert_eq!(decrypted, message);
}

#[test]
fn test_multiple_messages() {
    let (mut sender, mut receiver) = create_test_channel();

    let messages = vec![
        b"First message".to_vec(),
        b"Second message".to_vec(),
        b"Third message".to_vec(),
    ];

    // Encrypt all
    let mut encrypted = Vec::new();
    for msg in &messages {
        encrypted.extend_from_slice(&sender.encrypt(msg).unwrap());
    }

    // Feed all at once
    receiver.feed(&encrypted);

    // Decrypt all
    let decrypted = receiver.decrypt_all().unwrap();

    assert_eq!(decrypted.len(), 3);
    for (i, msg) in decrypted.iter().enumerate() {
        assert_eq!(msg, &messages[i]);
    }
}

#[test]
fn test_partial_frame() {
    let (mut sender, mut receiver) = create_test_channel();

    let message = b"Test partial frame";
    let encrypted = sender.encrypt(message).unwrap();

    // Feed only part of the frame
    receiver.feed(&encrypted[..5]);
    assert!(receiver.decrypt().unwrap().is_none());

    // Feed the rest
    receiver.feed(&encrypted[5..]);
    let decrypted = receiver.decrypt().unwrap().unwrap();

    assert_eq!(decrypted, message);
}

#[test]
fn test_nonce_increment() {
    let (mut sender, _) = create_test_channel();

    assert_eq!(sender.encrypt_nonce(), 0);

    sender.encrypt(b"message 1").unwrap();
    assert_eq!(sender.encrypt_nonce(), 1);

    sender.encrypt(b"message 2").unwrap();
    assert_eq!(sender.encrypt_nonce(), 2);
}

#[test]
fn test_disabled_passthrough() {
    let mut channel = EncryptedChannel::disabled();

    assert!(!channel.is_enabled());

    // Should pass through unchanged
    let message = b"Plaintext message";
    let encrypted = channel.encrypt(message).unwrap();
    assert_eq!(encrypted, message);

    channel.feed(message);
    let decrypted = channel.decrypt().unwrap().unwrap();
    assert_eq!(decrypted, message);
}

#[test]
fn test_wrong_key_fails() {
    let key_a = [0x41u8; 32];
    let key_b = [0x42u8; 32];
    let key_c = [0x43u8; 32];

    let mut sender = EncryptedChannel::new(key_a, key_b);
    let mut receiver = EncryptedChannel::new(key_a, key_c); // Wrong decrypt key

    let encrypted = sender.encrypt(b"Secret").unwrap();
    receiver.feed(&encrypted);

    // Decryption should fail authentication
    let result = receiver.decrypt();
    println!("Decryption result: {:?}", result);
    assert!(matches!(result, Err(EncryptionError::DecryptionFailed)));
}

#[test]
fn test_nonce_format() {
    use crate::protocol::crypto::Nonce;
    let nonce = Nonce::from_counter(0x0102030405060708);
    let nonce_bytes = nonce.as_bytes();

    // First 4 bytes zero, last 8 bytes are counter LE
    assert_eq!(nonce_bytes[0..4], [0, 0, 0, 0]);
    assert_eq!(
        nonce_bytes[4..12],
        [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
    );
}
