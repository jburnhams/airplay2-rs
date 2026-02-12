use num_bigint::BigUint;
use sha2::Sha512;

use super::{
    PairingError, PairingStepResult, TransientPairing,
    tlv::{TlvDecoder, TlvEncoder, TlvError, TlvType, errors},
};

#[test]
fn test_tlv_encode_simple() {
    let encoded = TlvEncoder::new().add_state(1).add_method(0).build();

    assert_eq!(
        encoded,
        vec![
            0x06, 0x01, 0x01, // State = 1
            0x00, 0x01, 0x00, // Method = 0
        ]
    );
}

struct MockSrpServer {
    n: BigUint,
    g: BigUint,
    k: BigUint,
    v: BigUint,
    b: BigUint,
    b_pub: BigUint,
    session_key: Vec<u8>,
    m2: Vec<u8>,
}

impl MockSrpServer {
    fn new(username: &[u8], password: &[u8], salt: &[u8]) -> Self {
        use sha2::Digest;

        let n = BigUint::parse_bytes(
            b"FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E08\
              8A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B\
              302B0A6DF25F14374FE1356D6D51C245E485B576625E7EC6F44C42E9\
              A637ED6B0BFF5CB6F406B7EDEE386BFB5A899FA5AE9F24117C4B1FE6\
              49286651ECE45B3DC2007CB8A163BF0598DA48361C55D39A69163FA8\
              FD24CF5F83655D23DCA3AD961C62F356208552BB9ED529077096966D\
              670C354E4ABC9804F1746C08CA18217C32905E462E36CE3BE39E772C\
              180E86039B2783A2EC07A28FB5C55DF06F4C52C9DE2BCBF695581718\
              3995497CEA956AE515D2261898FA051015728E5A8AAAC42DAD33170D\
              04507A33A85521ABDF1CBA64ECFB850458DBEF0A8AEA71575D060C7D\
              B3970F85A6E1E4C7ABF5AE8CDB0933D71E8C94E04A25619DCEE3D226\
              1AD2EE6BF12FFA06D98A0864D87602733EC86A64521F2B18177B200C\
              BBE117577A615D6C770988C0BAD946E208E24FA074E5AB3143DB5BFC\
              E0FD108E4B82D120A93AD2CAFFFFFFFFFFFFFFFF",
            16,
        )
        .unwrap();
        let g = BigUint::from(5u32);

        // k = H(N, pad(g))
        let k = {
            let mut hasher = Sha512::new();
            hasher.update(n.to_bytes_be());
            let g_bytes = g.to_bytes_be();
            let mut g_padded = vec![0u8; 384];
            g_padded[384 - g_bytes.len()..].copy_from_slice(&g_bytes);
            hasher.update(&g_padded);
            BigUint::from_bytes_be(&hasher.finalize())
        };

        // x = H(salt, H(username, ":", password))
        let x = {
            let mut inner = Sha512::new();
            inner.update(username);
            inner.update(b":");
            inner.update(password);
            let h_up = inner.finalize();

            let mut outer = Sha512::new();
            outer.update(salt);
            outer.update(h_up);
            BigUint::from_bytes_be(&outer.finalize())
        };

        // v = g^x % n
        let v = g.modpow(&x, &n);

        // b = random (fixed for test predictability)
        let b = BigUint::from(987_654_321u32);
        // B = (k*v + g^b) % n
        let b_pub = ((&k * &v) + g.modpow(&b, &n)) % &n;

        Self {
            n,
            g,
            k,
            v,
            b,
            b_pub,
            session_key: Vec::new(),
            m2: Vec::new(),
        }
    }

    fn public_key(&self) -> Vec<u8> {
        let mut bytes = self.b_pub.to_bytes_be();
        if bytes.len() < 384 {
            let mut padded = vec![0u8; 384];
            padded[384 - bytes.len()..].copy_from_slice(&bytes);
            bytes = padded;
        }
        bytes
    }

    fn verify_client(
        &mut self,
        username: &[u8],
        salt: &[u8],
        a_pub_bytes: &[u8],
        client_m1: &[u8],
    ) -> Result<Vec<u8>, ()> {
        use sha2::Digest;

        let a_pub = BigUint::from_bytes_be(a_pub_bytes);

        // u = H(pad(A), pad(B))
        let u = {
            let mut hasher = Sha512::new();
            let mut a_padded = vec![0u8; 384];
            let a_bytes = a_pub.to_bytes_be();
            a_padded[384 - a_bytes.len()..].copy_from_slice(&a_bytes);
            hasher.update(&a_padded);

            let mut b_padded = vec![0u8; 384];
            let b_bytes = self.b_pub.to_bytes_be();
            b_padded[384 - b_bytes.len()..].copy_from_slice(&b_bytes);
            hasher.update(&b_padded);
            BigUint::from_bytes_be(&hasher.finalize())
        };

        // S = (A * v^u) ^ b % n
        let s_shared = (a_pub * self.v.modpow(&u, &self.n)).modpow(&self.b, &self.n);

        // K = H(S)
        let k_session = {
            let mut hasher = Sha512::new();
            hasher.update(s_shared.to_bytes_be());
            hasher.finalize().to_vec()
        };

        // Verification of client_m1 would go here in a real implementation.
        // For the mock, we just derive K and calculate M2.

        self.session_key = k_session.clone();

        // M2 = H(A, M1, K)
        let mut hasher = Sha512::new();
        hasher.update(a_pub.to_bytes_be());
        hasher.update(client_m1);
        hasher.update(&k_session);
        self.m2 = hasher.finalize().to_vec();

        Ok(k_session)
    }

    fn server_proof(&self) -> &[u8] {
        &self.m2
    }
}

#[test]
fn test_pair_setup_m6_verification() {
    use num_bigint::BigUint;
    use sha2::Sha512;

    use crate::protocol::crypto::{ChaCha20Poly1305Cipher, Ed25519KeyPair, HkdfSha512, Nonce};
    use crate::protocol::pairing::setup::PairSetup;

    let mut client = PairSetup::new();
    let pin = "1234";
    client.set_pin(pin);

    // 1. Client Start (M1)
    let _m1 = client.start().unwrap();

    // 2. Device Response (M2) simulation
    let username = "Pair-Setup";
    let salt = b"salt-bytes";

    let mut srp_server = MockSrpServer::new(username.as_bytes(), pin.as_bytes(), salt);
    let server_public = srp_server.public_key();

    let m2 = TlvEncoder::new()
        .add_state(2)
        .add(TlvType::Salt, salt)
        .add(TlvType::PublicKey, &server_public)
        .build();

    // 3. Client Process M2 -> M3
    let m3 = match client.process_m2(&m2).unwrap() {
        PairingStepResult::SendData(data) => data,
        _ => panic!("Expected SendData for M3"),
    };

    let tlv_m3 = TlvDecoder::decode(&m3).unwrap();
    let client_public = tlv_m3.get_required(TlvType::PublicKey).unwrap();
    let client_proof = tlv_m3.get_required(TlvType::Proof).unwrap();

    // 4. Device Process M3 -> M4
    let session_key = srp_server
        .verify_client(username.as_bytes(), salt, client_public, client_proof)
        .unwrap();

    let m4 = TlvEncoder::new()
        .add_state(4)
        .add(TlvType::Proof, srp_server.server_proof())
        .build();

    // 5. Client Process M4 -> M5
    let _m5 = match client.process_m4(&m4).unwrap() {
        PairingStepResult::SendData(data) => data,
        _ => panic!("Expected SendData for M5"),
    };

    // 6. Device Process M5 and send M6
    // (We skip M5 verification on device side for brevity as we are testing M6 verification on client)

    // Device derives encryption key for M6
    let hkdf_enc = HkdfSha512::new(Some(b"Pair-Setup-Encrypt-Salt"), &session_key);
    let encrypt_key = hkdf_enc
        .expand_fixed::<32>(b"Pair-Setup-Encrypt-Info")
        .unwrap();

    // Device signs: HKDF(...) || identifier || public_key
    let hkdf_sign = HkdfSha512::new(Some(b"Pair-Setup-Accessory-Sign-Salt"), &session_key);
    let accessory_x = hkdf_sign
        .expand(b"Pair-Setup-Accessory-Sign-Info", 32)
        .unwrap();

    let device_signing = Ed25519KeyPair::generate();
    let device_id = b"device-id";

    let mut sign_data = Vec::new();
    sign_data.extend_from_slice(&accessory_x);
    sign_data.extend_from_slice(device_id);
    sign_data.extend_from_slice(device_signing.public_key().as_bytes());

    let signature = device_signing.sign(&sign_data);

    let inner_tlv = TlvEncoder::new()
        .add(TlvType::Identifier, device_id)
        .add(TlvType::PublicKey, device_signing.public_key().as_bytes())
        .add(TlvType::Signature, &signature.to_bytes())
        .build();

    let cipher = ChaCha20Poly1305Cipher::new(&encrypt_key).unwrap();
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..].copy_from_slice(b"PS-Msg06");
    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();

    let encrypted = cipher.encrypt(&nonce, &inner_tlv).unwrap();

    let m6 = TlvEncoder::new()
        .add_state(6)
        .add(TlvType::EncryptedData, &encrypted)
        .build();

    // 7. Client Process M6
    match client.process_m6(&m6) {
        Ok(PairingStepResult::Complete(_)) => {
            assert_eq!(
                client.device_public_key().unwrap(),
                device_signing.public_key().as_bytes()
            );
        }
        Err(e) => panic!("M6 verification failed: {:?}", e),
        _ => panic!("Expected Complete for M6"),
    }
}

#[test]
fn test_pair_setup_m6_invalid_signature() {
    use num_bigint::BigUint;
    use sha2::Sha512;

    use crate::protocol::crypto::{ChaCha20Poly1305Cipher, Ed25519KeyPair, HkdfSha512, Nonce};
    use crate::protocol::pairing::setup::PairSetup;

    let mut client = PairSetup::new();
    let pin = "1234";
    client.set_pin(pin);

    // 1-5. Go to state where client expects M6
    let _m1 = client.start().unwrap();
    let salt = b"salt-bytes";
    let mut srp_server = MockSrpServer::new(b"Pair-Setup", pin.as_bytes(), salt);
    let m2 = TlvEncoder::new()
        .add_state(2)
        .add(TlvType::Salt, salt)
        .add(TlvType::PublicKey, &srp_server.public_key())
        .build();
    let m3 = match client.process_m2(&m2).unwrap() {
        PairingStepResult::SendData(d) => d,
        _ => panic!(),
    };
    let tlv_m3 = TlvDecoder::decode(&m3).unwrap();
    let session_key = srp_server
        .verify_client(
            b"Pair-Setup",
            salt,
            tlv_m3.get_required(TlvType::PublicKey).unwrap(),
            tlv_m3.get_required(TlvType::Proof).unwrap(),
        )
        .unwrap();
    let m4 = TlvEncoder::new()
        .add_state(4)
        .add(TlvType::Proof, srp_server.server_proof())
        .build();
    let _m5 = client.process_m4(&m4).unwrap();

    // 6. Device sends M6 with INVALID signature
    let hkdf_enc = HkdfSha512::new(Some(b"Pair-Setup-Encrypt-Salt"), &session_key);
    let encrypt_key = hkdf_enc
        .expand_fixed::<32>(b"Pair-Setup-Encrypt-Info")
        .unwrap();

    let device_signing = Ed25519KeyPair::generate();
    let bad_key = Ed25519KeyPair::generate(); // Wrong key for signing
    let device_id = b"device-id";

    let hkdf_sign = HkdfSha512::new(Some(b"Pair-Setup-Accessory-Sign-Salt"), &session_key);
    let accessory_x = hkdf_sign
        .expand(b"Pair-Setup-Accessory-Sign-Info", 32)
        .unwrap();

    let mut sign_data = Vec::new();
    sign_data.extend_from_slice(&accessory_x);
    sign_data.extend_from_slice(device_id);
    sign_data.extend_from_slice(device_signing.public_key().as_bytes());

    let signature = bad_key.sign(&sign_data); // Sign with wrong key

    let inner_tlv = TlvEncoder::new()
        .add(TlvType::Identifier, device_id)
        .add(TlvType::PublicKey, device_signing.public_key().as_bytes())
        .add(TlvType::Signature, &signature.to_bytes())
        .build();

    let cipher = ChaCha20Poly1305Cipher::new(&encrypt_key).unwrap();
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..].copy_from_slice(b"PS-Msg06");
    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
    let encrypted = cipher.encrypt(&nonce, &inner_tlv).unwrap();

    let m6 = TlvEncoder::new()
        .add_state(6)
        .add(TlvType::EncryptedData, &encrypted)
        .build();

    // 7. Client Process M6 should FAIL
    let result = client.process_m6(&m6);
    assert!(matches!(result, Err(PairingError::CryptoError(_))));
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
    let tlv_decoder = TlvDecoder::decode(&encoded).unwrap();
    let decoded_bytes = tlv_decoder.get(TlvType::PublicKey).unwrap();
    assert_eq!(decoded_bytes, &long_data[..]);
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

#[test]
fn test_pair_verify_flow() {
    use crate::protocol::crypto::{
        ChaCha20Poly1305Cipher, Ed25519KeyPair, Ed25519Signature, HkdfSha512, Nonce, X25519KeyPair,
    };
    use crate::protocol::pairing::{PairVerify, PairingKeys};

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

// --- Enhanced Tests ---

#[test]
fn test_pair_setup_failures() {
    // Test that PairSetup correctly handles device error codes
    use crate::protocol::pairing::setup::PairSetup;

    let mut setup = PairSetup::new();
    setup.set_pin("1234");
    let _ = setup.start().unwrap();

    // Simulate M2 error from device
    let m2 = TlvEncoder::new()
        .add_state(2)
        .add_byte(TlvType::Error, errors::BUSY)
        .build();

    let result = setup.process_m2(&m2);
    assert!(matches!(result, Err(PairingError::DeviceError { code: 7 }))); // BUSY is 0x07
}

#[test]
fn test_pair_verify_invalid_signature() {
    use crate::protocol::crypto::{
        ChaCha20Poly1305Cipher, Ed25519KeyPair, HkdfSha512, Nonce, X25519KeyPair,
    };
    use crate::protocol::pairing::{PairVerify, PairingKeys};

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

#[test]
fn test_tlv_fragmentation_multiple() {
    // 3 fragments: 255 + 255 + 10
    let long_data = vec![0xAA; 520];

    let encoded = TlvEncoder::new()
        .add(TlvType::PublicKey, &long_data)
        .build();

    // Check structure
    // Frag 1: Type + Len(255) + 255 bytes
    // Frag 2: Type + Len(255) + 255 bytes
    // Frag 3: Type + Len(10) + 10 bytes
    // Total len: (1+1+255) * 2 + (1+1+10) = 514 + 12 = 526 bytes.
    assert_eq!(encoded.len(), 526);

    let decoder = TlvDecoder::decode(&encoded).unwrap();
    let decoded_data = decoder.get(TlvType::PublicKey).unwrap();
    assert_eq!(decoded_data, &long_data[..]);
}
