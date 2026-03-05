use crate::protocol::crypto::{
    ChaCha20Poly1305Cipher, Ed25519KeyPair, Ed25519Signature, HkdfSha512, Nonce, X25519KeyPair,
};
use crate::protocol::pairing::tlv::{TlvDecoder, TlvEncoder, TlvType};
use crate::protocol::pairing::{PairVerify, PairingError, PairingKeys, PairingStepResult};

#[test]
fn test_pair_verify_flow() {
    // 0. Setup existing keys (previously paired)
    let client_long_term = Ed25519KeyPair::generate();
    let device_long_term = Ed25519KeyPair::generate();

    let our_keys = PairingKeys {
        identifier: b"client-id".to_vec(),
        secret_key: client_long_term.secret_bytes(),
        public_key: *client_long_term.public_key().as_bytes(),
        device_public_key: *device_long_term.public_key().as_bytes(),
    };

    let mut client = PairVerify::new(our_keys, device_long_term.public_key().as_bytes()).unwrap();

    // 1. Client Start (M1)
    let m1 = client.start().unwrap();
    let tlv_m1 = TlvDecoder::decode(&m1).unwrap();
    let client_ephemeral_bytes = tlv_m1.get_required(TlvType::PublicKey).unwrap();
    let client_ephemeral =
        crate::protocol::crypto::X25519PublicKey::from_bytes(client_ephemeral_bytes).unwrap();

    // 2. Device Process M1 -> M2
    let device_ephemeral = X25519KeyPair::generate();
    let shared = device_ephemeral.diffie_hellman(&client_ephemeral);

    let hkdf = HkdfSha512::new(Some(b"Pair-Verify-Encrypt-Salt"), shared.as_bytes());
    let session_key = hkdf
        .expand_fixed::<32>(b"Pair-Verify-Encrypt-Info")
        .unwrap();

    // Device signs: device_ephemeral || client_ephemeral
    let mut sign_data = Vec::new();
    sign_data.extend_from_slice(device_ephemeral.public_key().as_bytes());
    sign_data.extend_from_slice(client_ephemeral_bytes);
    let signature = device_long_term.sign(&sign_data);

    // Encrypt: identifier + signature
    let inner_tlv = TlvEncoder::new()
        .add(TlvType::Identifier, b"device-id")
        .add(TlvType::Signature, &signature.to_bytes())
        .build();

    let cipher = ChaCha20Poly1305Cipher::new(&session_key).unwrap();
    // Use "PV-Msg02" as nonce
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..].copy_from_slice(b"PV-Msg02");
    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
    let encrypted = cipher.encrypt(&nonce, &inner_tlv).unwrap();

    let m2 = TlvEncoder::new()
        .add_state(2)
        .add(TlvType::PublicKey, device_ephemeral.public_key().as_bytes())
        .add(TlvType::EncryptedData, &encrypted)
        .build();

    // 3. Client Process M2 -> M3
    match client.process_m2(&m2) {
        Ok(PairingStepResult::SendData(m3)) => {
            // 4. Device processes M3
            let tlv_m3 = TlvDecoder::decode(&m3).unwrap();
            let m3_encrypted = tlv_m3.get_required(TlvType::EncryptedData).unwrap();

            // Decrypt M3
            // Use "PV-Msg03" as nonce
            let mut nonce_bytes = [0u8; 12];
            nonce_bytes[4..].copy_from_slice(b"PV-Msg03");
            let nonce_m3 = Nonce::from_bytes(&nonce_bytes).unwrap();
            let decrypted_m3 = cipher
                .decrypt(&nonce_m3, m3_encrypted)
                .expect("Device failed to decrypt M3");

            let tlv_inner = TlvDecoder::decode(&decrypted_m3).unwrap();
            let client_sig_bytes = tlv_inner.get_required(TlvType::Signature).unwrap();

            // Verify client signature: client_ephemeral || device_ephemeral
            let mut verify_data = Vec::new();
            verify_data.extend_from_slice(client_ephemeral_bytes);
            verify_data.extend_from_slice(device_ephemeral.public_key().as_bytes());

            let client_sig = Ed25519Signature::from_bytes(client_sig_bytes).unwrap();
            client_long_term
                .public_key()
                .verify(&verify_data, &client_sig)
                .unwrap();

            // 5. Device sends M4
            let m4 = TlvEncoder::new().add_state(4).build();

            match client.process_m4(&m4) {
                Ok(PairingStepResult::Complete(_)) => {}
                _ => panic!("Expected Complete"),
            }
        }
        _ => panic!("Expected SendData for M3"),
    }
}

#[test]
fn test_pair_verify_invalid_signature() {
    // Setup keys
    let client_long_term = Ed25519KeyPair::generate();
    let device_long_term = Ed25519KeyPair::generate();

    let our_keys = PairingKeys {
        identifier: b"client-id".to_vec(),
        secret_key: client_long_term.secret_bytes(),
        public_key: *client_long_term.public_key().as_bytes(),
        device_public_key: *device_long_term.public_key().as_bytes(),
    };

    let mut client = PairVerify::new(our_keys, device_long_term.public_key().as_bytes()).unwrap();
    let m1 = client.start().unwrap();
    let tlv_m1 = TlvDecoder::decode(&m1).unwrap();
    let client_ephemeral_bytes = tlv_m1.get_required(TlvType::PublicKey).unwrap();
    let client_ephemeral =
        crate::protocol::crypto::X25519PublicKey::from_bytes(client_ephemeral_bytes).unwrap();

    // Device side simulation
    let device_ephemeral = X25519KeyPair::generate();
    let shared = device_ephemeral.diffie_hellman(&client_ephemeral);

    let hkdf = HkdfSha512::new(Some(b"Pair-Verify-Encrypt-Salt"), shared.as_bytes());
    let session_key = hkdf
        .expand_fixed::<32>(b"Pair-Verify-Encrypt-Info")
        .unwrap();

    // Device signs: device_ephemeral || client_ephemeral
    let mut sign_data = Vec::new();
    sign_data.extend_from_slice(device_ephemeral.public_key().as_bytes());
    sign_data.extend_from_slice(client_ephemeral_bytes);

    // !!! Malicious device uses wrong key to sign !!!
    let bad_key = Ed25519KeyPair::generate();
    let signature = bad_key.sign(&sign_data);

    // Encrypt
    let inner_tlv = TlvEncoder::new()
        .add(TlvType::Identifier, b"device-id")
        .add(TlvType::Signature, &signature.to_bytes())
        .build();

    let cipher = ChaCha20Poly1305Cipher::new(&session_key).unwrap();
    let nonce = Nonce::from_bytes(&[0u8; 12]).unwrap();
    let encrypted = cipher.encrypt(&nonce, &inner_tlv).unwrap();

    let m2 = TlvEncoder::new()
        .add_state(2)
        .add(TlvType::PublicKey, device_ephemeral.public_key().as_bytes())
        .add(TlvType::EncryptedData, &encrypted)
        .build();

    // Client process M2 should fail signature verification
    let result = client.process_m2(&m2);
    assert!(matches!(result, Err(PairingError::CryptoError(_))));
}
