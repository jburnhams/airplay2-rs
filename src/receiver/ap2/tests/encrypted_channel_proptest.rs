use crate::receiver::ap2::encrypted_channel::EncryptedChannel;
use proptest::prelude::*;

proptest! {
    // Roundtrip encryption/decryption with random keys and messages
    #[test]
    fn test_encrypt_decrypt_roundtrip(
        key_a in proptest::collection::vec(any::<u8>(), 32),
        key_b in proptest::collection::vec(any::<u8>(), 32),
        message in proptest::collection::vec(any::<u8>(), 0..2048) // Max typical frame
    ) {
        let key_a_arr: [u8; 32] = key_a.try_into().unwrap();
        let key_b_arr: [u8; 32] = key_b.try_into().unwrap();

        // Sender uses A to encrypt (to receiver)
        // Receiver uses A to decrypt (from sender)
        let mut sender = EncryptedChannel::new(key_a_arr, key_b_arr);
        let mut receiver = EncryptedChannel::new(key_b_arr, key_a_arr);

        let encrypted = sender.encrypt(&message).unwrap();

        receiver.feed(&encrypted);
        let decrypted = receiver.decrypt().unwrap().unwrap();

        assert_eq!(decrypted, message);
    }

    // Fuzz decryption with random bytes (simulating garbage on the wire)
    #[test]
    fn test_decrypt_random_garbage(
        key in proptest::collection::vec(any::<u8>(), 32),
        garbage in proptest::collection::vec(any::<u8>(), 0..1024)
    ) {
        let key_arr: [u8; 32] = key.try_into().unwrap();
        let mut receiver = EncryptedChannel::new(key_arr, key_arr);

        receiver.feed(&garbage);
        // It should either return Ok(None) (incomplete) or Err (invalid/auth fail)
        // But crucially, it should NOT panic
        let _ = receiver.decrypt();
    }
}
