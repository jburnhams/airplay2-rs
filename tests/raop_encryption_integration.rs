use airplay2::protocol::crypto::RaopRsaPrivateKey;
use airplay2::protocol::raop::encryption::{
    EncryptionConfig, EncryptionMode, FRAME_SIZE, RaopDecryptor, RaopEncryptor,
};

#[test]
fn test_key_exchange_simulation() {
    use airplay2::protocol::crypto::CompatibleOsRng;
    use rand::RngCore;
    use rand::rngs::OsRng;
    use rsa::Oaep;
    use sha1::Sha1;

    // Server generates RSA key pair (Receiver side)
    let server_key = RaopRsaPrivateKey::generate().unwrap();

    // Client generates session keys (Sender side)
    let mut client_aes_key = [0u8; 16];
    let mut client_aes_iv = [0u8; 16];
    let mut rng = rand::thread_rng();
    rng.fill_bytes(&mut client_aes_key);
    rng.fill_bytes(&mut client_aes_iv);

    // Client encrypts AES key with server's public key (Simulating what AirPlay Sender does)
    let public = server_key.public_key();
    let padding = Oaep::<Sha1>::new();

    // Using rsa::Encryptor trait method
    let mut rng = CompatibleOsRng(OsRng);
    let encrypted_key = public
        .encrypt(&mut rng, padding, &client_aes_key)
        .unwrap();

    // Server decrypts to get AES key
    let decrypted_key = server_key.decrypt_oaep(&encrypted_key).unwrap();

    assert_eq!(decrypted_key, client_aes_key);

    // Both sides can now encrypt/decrypt
    let client_encryptor = RaopEncryptor::new(client_aes_key, client_aes_iv);
    // Note: RaopDecryptor takes array, we have array.
    let server_decryptor = RaopDecryptor::new(client_aes_key, client_aes_iv);

    let test_audio = vec![0x55u8; FRAME_SIZE];
    let encrypted = client_encryptor.encrypt(&test_audio, 0).unwrap();
    let decrypted = server_decryptor.decrypt(&encrypted, 0).unwrap();

    assert_eq!(decrypted, test_audio);
}

#[test]
fn test_unencrypted_mode() {
    let config = EncryptionConfig::unencrypted();

    assert_eq!(config.mode, EncryptionMode::None);
    assert!(!config.is_encrypted());

    let encryptor = config.encryptor().unwrap();
    assert!(!encryptor.is_enabled());

    let data = vec![0xAA; 100];
    let result = encryptor.encrypt(&data, 0).unwrap();
    assert_eq!(result, data);
}
