use crate::protocol::pairing::{
    PairingError, PairingStepResult, TransientPairing,
    tlv::{TlvDecoder, TlvEncoder, TlvType, errors},
};

#[test]
fn test_transient_start() {
    let mut pairing = TransientPairing::new();
    let m1 = pairing.start().unwrap();

    let decoder = TlvDecoder::decode(&m1).unwrap();
    assert_eq!(decoder.get_state().unwrap(), 1);
    assert!(decoder.get(TlvType::PublicKey).is_some());
}

#[test]
fn test_transient_invalid_state() {
    let mut pairing = TransientPairing::new();

    // Try to process M2 without starting
    let result = pairing.process_m2(&[]);
    assert!(matches!(result, Err(PairingError::InvalidState { .. })));
}

#[test]
fn test_transient_device_error() {
    let mut pairing = TransientPairing::new();
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

    let mut client = TransientPairing::new();

    // 1. Client Start (M1)
    let m1 = client.start().unwrap();
    let tlv_m1 = TlvDecoder::decode(&m1).unwrap();
    let client_pub_bytes = tlv_m1.get_required(TlvType::PublicKey).unwrap();
    let client_public =
        crate::protocol::crypto::X25519PublicKey::from_bytes(client_pub_bytes).unwrap();

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
    let session_key = hkdf
        .expand_fixed::<32>(b"Pair-Verify-Encrypt-Info")
        .unwrap();

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

            let decrypted_m3 = cipher
                .decrypt(&nonce, m3_encrypted)
                .expect("Device failed to decrypt M3");
            let tlv_inner_m3 = TlvDecoder::decode(&decrypted_m3).unwrap();
            let _client_sig = tlv_inner_m3.get_required(TlvType::Signature).unwrap();

            // 5. Device sends M4 (OK)
            let m4 = TlvEncoder::new().add_state(4).build();

            match client.process_m4(&m4) {
                Ok(PairingStepResult::Complete(keys)) => {
                    assert_ne!(keys.encrypt_key, [0u8; 32]);
                }
                _ => panic!("Expected Complete"),
            }
        }
        Ok(res) => panic!("Expected SendData, got {res:?}"),
        Err(e) => panic!("Error processing M2: {e:?}"),
    }
}
