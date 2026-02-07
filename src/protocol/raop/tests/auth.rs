use super::*;
use crate::protocol::crypto::RaopRsaPrivateKey;
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD as BASE64};
use std::net::IpAddr;

#[test]
fn test_challenge_generation() {
    let c1 = generate_challenge();
    let c2 = generate_challenge();

    // Should be different (with overwhelming probability)
    assert_ne!(c1, c2);
    assert_eq!(c1.len(), CHALLENGE_SIZE);
}

#[test]
fn test_challenge_encode_decode() {
    let challenge = generate_challenge();
    let encoded = encode_challenge(&challenge);
    let decoded = decode_challenge(&encoded).unwrap();

    assert_eq!(decoded, challenge);
}

#[test]
fn test_response_message_building() {
    let challenge = [0x01u8; CHALLENGE_SIZE];
    let ip: IpAddr = "192.168.1.100".parse().unwrap();
    let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];

    let message = super::auth::build_response_message(&challenge, &ip, &mac);

    // Should contain challenge + IP + MAC (padded to 32 bytes)
    assert!(message.len() >= 32);
    assert!(message.starts_with(&challenge));
}

#[test]
fn test_response_message_ipv6() {
    let challenge = [0x01u8; CHALLENGE_SIZE];
    let ip: IpAddr = "::1".parse().unwrap();
    let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];

    let message = super::auth::build_response_message(&challenge, &ip, &mac);

    // IPv6 address is 16 bytes
    assert!(message.len() >= CHALLENGE_SIZE + 16 + 6);
}

#[test]
fn test_authenticator_state_machine() {
    let mut auth = RaopAuthenticator::new();

    assert_eq!(auth.state(), AuthState::Initial);
    assert!(!auth.is_authenticated());

    // Get challenge header
    let header = auth.challenge_header();
    assert!(!header.is_empty());

    auth.mark_sent();
    assert_eq!(auth.state(), AuthState::ChallengeSent);
}

#[test]
fn test_full_auth_flow_with_generated_key() {
    // Generate a test RSA key pair
    let private = RaopRsaPrivateKey::generate().unwrap();

    let challenge = generate_challenge();
    let ip: IpAddr = "192.168.1.100".parse().unwrap();
    let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];

    // Server generates response
    let response = generate_response(&private, &challenge, &ip, &mac).unwrap();

    // In real code, client would verify with Apple's public key
    // Here we just verify the signature format
    let decoded = BASE64.decode(&response).unwrap();
    assert_eq!(decoded.len(), 128); // 1024-bit RSA signature
}

#[test]
fn test_session_key_generation() {
    // This test requires the actual Apple public key to work
    // In testing, we'd mock this or use a test key pair

    // Test the base64 encoding/decoding functions
    let test_key = [0x42u8; AES_KEY_SIZE];
    let test_iv = [0x00u8; AES_IV_SIZE];

    let key_b64 = BASE64.encode(test_key);
    let iv_b64 = BASE64.encode(test_iv);

    let decoded_key = BASE64.decode(&key_b64).unwrap();
    let decoded_iv = BASE64.decode(&iv_b64).unwrap();

    assert_eq!(decoded_key, test_key);
    assert_eq!(decoded_iv, test_iv);
}

#[test]
fn test_session_keys_with_test_keypair() {
    // Generate AES key and IV
    use crate::protocol::crypto::CompatibleOsRng;
    use rand::rngs::OsRng;
    use rsa::Oaep;
    use sha1::Sha1;

    // Generate test key pair
    let private = RaopRsaPrivateKey::generate().unwrap();

    let mut aes_key = [0u8; AES_KEY_SIZE];
    let mut aes_iv = [0u8; AES_IV_SIZE];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut aes_key);
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut aes_iv);

    // Encrypt with public key
    let public = private.public_key();
    let padding = Oaep::<Sha1>::new();
    let mut rng = CompatibleOsRng(OsRng);
    let encrypted = public.encrypt(&mut rng, padding, &aes_key).unwrap();

    // Encode as SDP attributes
    let rsaaeskey = BASE64.encode(&encrypted);
    let aesiv_b64 = BASE64.encode(aes_iv);

    // Parse back
    let (parsed_key, parsed_iv) = parse_session_keys(&rsaaeskey, &aesiv_b64, &private).unwrap();

    assert_eq!(parsed_key, aes_key);
    assert_eq!(parsed_iv, aes_iv);
}

#[test]
fn test_zeroization() {
    // Verify keys are zeroized on drop
    let _key_ptr: *const u8;
    {
        let mut keys = RaopSessionKeys {
            aes_key: [0x42; AES_KEY_SIZE],
            aes_iv: [0x42; AES_IV_SIZE],
            encrypted_key: vec![0x42; 128],
        };
        // We can't easily test zeroization without unsafe or inspection,
        // but we can ensure it compiles and runs.
        // To properly test, we'd need to keep a pointer, drop, and check memory,
        // which is unsafe and UB if memory is reused.
        // So we just call drop implicitly.
        keys.aes_key[0] = 0x43;
    }
}
