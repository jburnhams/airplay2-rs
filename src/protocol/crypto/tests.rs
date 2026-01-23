use super::*;
use rand::RngCore;

// --- aes.rs tests ---

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

// --- chacha.rs tests ---

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

// --- ed25519.rs tests ---

#[test]
fn test_ed25519_keypair_generation() {
    let kp = Ed25519KeyPair::generate();
    let pk = kp.public_key();

    assert_eq!(pk.as_bytes().len(), 32);
}

#[test]
fn test_ed25519_keypair_from_bytes() {
    let kp1 = Ed25519KeyPair::generate();
    let secret = kp1.secret_bytes();

    let kp2 = Ed25519KeyPair::from_bytes(&secret).unwrap();

    assert_eq!(kp1.public_key().as_bytes(), kp2.public_key().as_bytes());
}

#[test]
fn test_ed25519_sign_verify() {
    let kp = Ed25519KeyPair::generate();
    let message = b"test message";

    let signature = kp.sign(message);
    kp.public_key().verify(message, &signature).unwrap();
}

#[test]
fn test_ed25519_verify_wrong_message() {
    let kp = Ed25519KeyPair::generate();

    let signature = kp.sign(b"original message");
    let result = kp.public_key().verify(b"different message", &signature);

    assert!(matches!(result, Err(CryptoError::InvalidSignature)));
}

#[test]
fn test_ed25519_signature_roundtrip() {
    let kp = Ed25519KeyPair::generate();
    let signature = kp.sign(b"message");

    let bytes = signature.to_bytes();
    let recovered = Ed25519Signature::from_bytes(&bytes).unwrap();

    kp.public_key().verify(b"message", &recovered).unwrap();
}

// --- hkdf.rs tests ---

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

// --- srp.rs tests ---

#[test]
fn test_srp_client_creation() {
    let client = SrpClient::new().unwrap();
    assert!(!client.public_key().is_empty());
}

#[test]
fn test_srp_handshake() {
    // 1. Client setup
    let client = SrpClient::new().unwrap();
    let username = b"Pair-Setup";
    let password = b"1234";
    let client_a = client.public_key();

    // 2. Server setup (simulation)
    let salt = b"randomsalt";

    // Use Client to compute verifier (simulating registration)
    let helper_client = ::srp::client::SrpClient::<sha2::Sha512>::new(&::srp::groups::G_3072);
    let verifier = helper_client.compute_verifier(username, password, salt);

    let server = ::srp::server::SrpServer::<sha2::Sha512>::new(&::srp::groups::G_3072);

    // Server generates ephemeral B
    let mut b_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b_bytes);
    let server_b_pub = server.compute_public_ephemeral(&b_bytes, &verifier);

    // 3. Client processes challenge
    let client_verifier = client
        .process_challenge(username, password, salt, &server_b_pub)
        .expect("Client failed to process challenge");

    // 4. Client generates proof
    let client_m1 = client_verifier.client_proof();

    // 5. Server verifies client
    let server_verifier = server
        .process_reply(&b_bytes, &verifier, client_a)
        .expect("Server failed to process reply");

    server_verifier
        .verify_client(client_m1)
        .expect("Server failed to verify client");
    let server_key = server_verifier.key();

    let server_m2 = server_verifier.proof();

    // 6. Client verifies server
    let client_key = client_verifier
        .verify_server(server_m2)
        .expect("Client failed to verify server");

    assert_eq!(client_key.as_bytes(), server_key);
}

#[test]
fn test_srp_invalid_password_fails() {
    let client = SrpClient::new().unwrap();
    let username = b"Pair-Setup";
    let password = b"correct";
    let salt = b"salt";

    // Helper for registration
    let helper_client = ::srp::client::SrpClient::<sha2::Sha512>::new(&::srp::groups::G_3072);
    // Server registered with "wrong" password
    let verifier = helper_client.compute_verifier(username, b"wrong", salt);

    let server = ::srp::server::SrpServer::<sha2::Sha512>::new(&::srp::groups::G_3072);
    let mut b_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b_bytes);
    let server_b_pub = server.compute_public_ephemeral(&b_bytes, &verifier);

    // Client tries with "correct" password
    let client_verifier = client
        .process_challenge(username, password, salt, &server_b_pub)
        .unwrap();

    let client_m1 = client_verifier.client_proof();

    let server_verifier = server
        .process_reply(&b_bytes, &verifier, client.public_key())
        .unwrap();

    // Verification should fail
    assert!(server_verifier.verify_client(client_m1).is_err());
}

// --- x25519.rs tests ---

#[test]
fn test_x25519_key_exchange() {
    let alice = X25519KeyPair::generate();
    let bob = X25519KeyPair::generate();

    let alice_shared = alice.diffie_hellman(&bob.public_key());
    let bob_shared = bob.diffie_hellman(&alice.public_key());

    assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
}

#[test]
fn test_x25519_keypair_roundtrip() {
    let kp1 = X25519KeyPair::generate();
    let secret = kp1.secret_bytes();

    let kp2 = X25519KeyPair::from_bytes(&secret).unwrap();

    assert_eq!(kp1.public_key().as_bytes(), kp2.public_key().as_bytes());
}

#[test]
fn test_x25519_public_key_from_bytes() {
    let kp = X25519KeyPair::generate();
    let pk_bytes = kp.public_key().as_bytes().to_vec();

    let pk = X25519PublicKey::from_bytes(&pk_bytes).unwrap();

    assert_eq!(pk.as_bytes(), kp.public_key().as_bytes());
}
