use super::{
    tlv::{TlvDecoder, TlvEncoder, TlvError, TlvType, errors},
    PairingError, TransientPairing, PairingStepResult,
};

#[test]
fn test_tlv_encode_simple() {
    let encoded = TlvEncoder::new()
        .add_state(1)
        .add_method(0)
        .build();

    assert_eq!(encoded, vec![
        0x06, 0x01, 0x01,  // State = 1
        0x00, 0x01, 0x00,  // Method = 0
    ]);
}

#[test]
fn test_tlv_decode_simple() {
    let data = vec![0x06, 0x01, 0x01, 0x00, 0x01, 0x00];
    let decoder = TlvDecoder::decode(&data).unwrap();

    assert_eq!(decoder.get_state().unwrap(), 1);
    assert_eq!(decoder.get(TlvType::Method), Some(&[0u8][..]));
}

#[test]
fn test_tlv_fragmentation() {
    // Data longer than 255 bytes should be fragmented
    let long_data = vec![0xAA; 300];
    let encoded = TlvEncoder::new()
        .add(TlvType::PublicKey, &long_data)
        .build();

    // Should have two TLV entries
    assert_eq!(encoded[0], TlvType::PublicKey as u8);
    assert_eq!(encoded[1], 255); // First chunk is max size
    // 255 bytes of data
    // Then next chunk
    assert_eq!(encoded[255 + 2], TlvType::PublicKey as u8);
    assert_eq!(encoded[255 + 3], 45); // 300 - 255 = 45

    // Decode should reassemble
    let decoder = TlvDecoder::decode(&encoded).unwrap();
    let decoded = decoder.get(TlvType::PublicKey).unwrap();
    assert_eq!(decoded, &long_data[..]);
}

#[test]
fn test_tlv_error_detection() {
    let data = vec![0x07, 0x01, 0x02]; // Error = 2
    let decoder = TlvDecoder::decode(&data).unwrap();

    assert!(decoder.has_error());
    assert_eq!(decoder.get_error(), Some(2));
}

#[test]
fn test_tlv_missing_field() {
    let data = vec![0x06, 0x01, 0x01]; // Only state
    let decoder = TlvDecoder::decode(&data).unwrap();

    let result = decoder.get_required(TlvType::PublicKey);
    assert!(matches!(result, Err(TlvError::MissingField(_))));
}

#[test]
fn test_transient_start() {
    let mut pairing = TransientPairing::new().unwrap();
    let m1 = pairing.start().unwrap();

    let decoder = TlvDecoder::decode(&m1).unwrap();
    assert_eq!(decoder.get_state().unwrap(), 1);
    assert!(decoder.get(TlvType::PublicKey).is_some());
}

#[test]
fn test_transient_invalid_state() {
    let mut pairing = TransientPairing::new().unwrap();

    // Try to process M2 without starting
    let result = pairing.process_m2(&[]);
    assert!(matches!(result, Err(PairingError::InvalidState { .. })));
}

#[test]
fn test_transient_device_error() {
    let mut pairing = TransientPairing::new().unwrap();
    pairing.start().unwrap();

    // Simulate device error response
    let m2 = TlvEncoder::new()
        .add_state(2)
        .add_byte(TlvType::Error, errors::AUTHENTICATION)
        .build();

    let result = pairing.process_m2(&m2);
    assert!(matches!(result, Err(PairingError::DeviceError { code: 2 })));
}

#[test]
fn test_transient_pairing_flow() {
    // This tests the client side of Transient Pairing.
    // To test properly, we need to simulate the Device side.

    let mut client = TransientPairing::new().unwrap();

    // 1. Client Start (M1)
    let m1 = client.start().unwrap();
    let tlv_m1 = TlvDecoder::decode(&m1).unwrap();
    let client_pub_bytes = tlv_m1.get_required(TlvType::PublicKey).unwrap();
    let client_public = crate::protocol::crypto::X25519PublicKey::from_bytes(client_pub_bytes).unwrap();

    // 2. Device Response (M2) simulation
    // Device generates its own keypair
    let device_keypair = crate::protocol::crypto::X25519KeyPair::generate();
    let device_signing = crate::protocol::crypto::Ed25519KeyPair::generate();

    // Device computes shared secret
    let shared_secret = device_keypair.diffie_hellman(&client_public);

    // Device derives session keys
    let hkdf = crate::protocol::crypto::HkdfSha512::new(
        Some(b"Pair-Verify-Encrypt-Salt"),
        shared_secret.as_bytes(),
    );
    let session_key = hkdf.expand_fixed::<32>(b"Pair-Verify-Encrypt-Info").unwrap();

    // Device signs: device_public || client_public
    let mut proof_data = Vec::new();
    proof_data.extend_from_slice(device_keypair.public_key().as_bytes());
    proof_data.extend_from_slice(client_pub_bytes);
    let signature = device_signing.sign(&proof_data);

    // Device Encrypts: identifier + signature
    let inner_tlv = TlvEncoder::new()
        .add(TlvType::Identifier, b"device-id")
        .add(TlvType::Signature, &signature.to_bytes())
        .build();

    let cipher = crate::protocol::crypto::ChaCha20Poly1305Cipher::new(&session_key).unwrap();
    let nonce = crate::protocol::crypto::Nonce::from_bytes(&[0u8; 12]).unwrap();
    let encrypted = cipher.encrypt(&nonce, &inner_tlv).unwrap();

    let m2 = TlvEncoder::new()
        .add_state(2)
        .add(TlvType::PublicKey, device_keypair.public_key().as_bytes())
        .add(TlvType::EncryptedData, &encrypted)
        .build();

    // 3. Client Process M2 -> M3
    match client.process_m2(&m2) {
        Ok(PairingStepResult::SendData(m3)) => {
            // 4. Device processes M3
            let tlv_m3 = TlvDecoder::decode(&m3).unwrap();
            assert_eq!(tlv_m3.get_state().unwrap(), 3);
            let m3_encrypted = tlv_m3.get_required(TlvType::EncryptedData).unwrap();

            // Device decrypts M3
            // Note: client uses same session key for M3 encryption?
            // "The session key is derived from the shared secret."
            // Both sides derive same session key.
            // But nonce might be different?
            // "nonce = Nonce::from_bytes(&[0u8; 12])?" in Client code.
            // If Client uses same nonce as Device used for M2, we have a problem (reuse).
            // But this is Transient Pairing "Pair-Setup" or "Pair-Verify"?
            // Transient pairing seems to mimic Pair-Verify structure.
            // Client used nonce 0. Device used nonce 0. This is bad for security if same key.
            // But this is implementing spec.

            let decrypted_m3 = cipher.decrypt(&nonce, m3_encrypted).expect("Device failed to decrypt M3");
            let tlv_inner_m3 = TlvDecoder::decode(&decrypted_m3).unwrap();
            let _client_sig = tlv_inner_m3.get_required(TlvType::Signature).unwrap();

            // 5. Device sends M4 (OK)
            let m4 = TlvEncoder::new()
                .add_state(4)
                .build();

            match client.process_m4(&m4) {
                Ok(PairingStepResult::Complete(keys)) => {
                    assert_ne!(keys.encrypt_key, [0u8; 32]);
                },
                _ => panic!("Expected Complete"),
            }
        },
        Ok(res) => panic!("Expected SendData, got {:?}", res),
        Err(e) => panic!("Error processing M2: {:?}", e),
    }
}

#[test]
fn test_pair_verify_flow() {
    use crate::protocol::pairing::{PairVerify, PairingKeys};
    use crate::protocol::crypto::{Ed25519KeyPair, X25519KeyPair, HkdfSha512, ChaCha20Poly1305Cipher, Nonce, Ed25519Signature};

    // 0. Setup existing keys (previously paired)
    let client_long_term = Ed25519KeyPair::generate();
    let device_long_term = Ed25519KeyPair::generate();

    let our_keys = PairingKeys {
        identifier: b"client-id".to_vec(),
        secret_key: client_long_term.secret_bytes(),
        public_key: client_long_term.public_key().as_bytes().clone(),
        device_public_key: device_long_term.public_key().as_bytes().clone(),
    };

    let mut client = PairVerify::new(our_keys, device_long_term.public_key().as_bytes()).unwrap();

    // 1. Client Start (M1)
    let m1 = client.start().unwrap();
    let tlv_m1 = TlvDecoder::decode(&m1).unwrap();
    let client_ephemeral_bytes = tlv_m1.get_required(TlvType::PublicKey).unwrap();
    let client_ephemeral = crate::protocol::crypto::X25519PublicKey::from_bytes(client_ephemeral_bytes).unwrap();

    // 2. Device Process M1 -> M2
    let device_ephemeral = X25519KeyPair::generate();
    let shared = device_ephemeral.diffie_hellman(&client_ephemeral);

    let hkdf = HkdfSha512::new(
        Some(b"Pair-Verify-Encrypt-Salt"),
        shared.as_bytes(),
    );
    let session_key = hkdf.expand_fixed::<32>(b"Pair-Verify-Encrypt-Info").unwrap();

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
    let nonce = Nonce::from_bytes(&[0u8; 12]).unwrap();
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
             // Spec says client uses nonce [0..01] for M3?
             // "let nonce = Nonce::from_bytes(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])?;"
             let nonce_m3 = Nonce::from_bytes(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]).unwrap();
             let decrypted_m3 = cipher.decrypt(&nonce_m3, m3_encrypted).expect("Device failed to decrypt M3");

             let tlv_inner = TlvDecoder::decode(&decrypted_m3).unwrap();
             let client_sig_bytes = tlv_inner.get_required(TlvType::Signature).unwrap();

             // Verify client signature: client_ephemeral || device_ephemeral
             let mut verify_data = Vec::new();
             verify_data.extend_from_slice(client_ephemeral_bytes);
             verify_data.extend_from_slice(device_ephemeral.public_key().as_bytes());

             let client_sig = Ed25519Signature::from_bytes(client_sig_bytes).unwrap();
             client_long_term.public_key().verify(&verify_data, &client_sig).unwrap();

             // 5. Device sends M4
             let m4 = TlvEncoder::new()
                .add_state(4)
                .build();

             match client.process_m4(&m4) {
                 Ok(PairingStepResult::Complete(_)) => {},
                 _ => panic!("Expected Complete"),
             }
        },
        _ => panic!("Expected SendData for M3"),
    }
}
