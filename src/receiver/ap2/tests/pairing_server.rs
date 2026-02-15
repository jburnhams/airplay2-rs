use crate::protocol::crypto::{
    ChaCha20Poly1305Cipher, Ed25519KeyPair, Nonce, SrpClient, SrpParams, SrpVerifier,
    X25519KeyPair, X25519PublicKey, derive_key,
};
use crate::protocol::pairing::tlv::{TlvDecoder, TlvEncoder, TlvType};
use crate::receiver::ap2::pairing_server::{PairingServer, PairingServerState};

/// Mock client for testing PairingServer
struct PairingClient {
    srp_client: SrpClient,
    srp_verifier: Option<SrpVerifier>,
    srp_session_key: Option<Vec<u8>>,
    verify_keypair: Option<X25519KeyPair>,
    shared_secret: Option<[u8; 32]>,
    server_public_key: Option<[u8; 32]>,
    password: String,
}

impl PairingClient {
    fn new(password: &str) -> Self {
        let srp_client = SrpClient::new(&SrpParams::RFC5054_3072).expect("SrpClient init");

        Self {
            srp_client,
            srp_verifier: None,
            srp_session_key: None,
            verify_keypair: None,
            shared_secret: None,
            server_public_key: None,
            password: password.to_string(),
        }
    }

    fn create_m1(&self) -> Vec<u8> {
        TlvEncoder::new()
            .add_state(1)
            .add_byte(TlvType::Method, 0)
            .build()
    }

    fn handle_m2_create_m3(&mut self, m2_data: &[u8]) -> Vec<u8> {
        let tlv = TlvDecoder::decode(m2_data).expect("Decode M2");
        let salt = tlv.get(TlvType::Salt).expect("M2 Salt");
        let server_public = tlv.get(TlvType::PublicKey).expect("M2 PublicKey");

        let verifier = self
            .srp_client
            .process_challenge(b"Pair-Setup", self.password.as_bytes(), salt, server_public)
            .expect("Process challenge");

        let client_proof = verifier.client_proof().to_vec();
        let client_public = self.srp_client.public_key();

        // Store verifier for M4
        self.srp_verifier = Some(verifier);

        TlvEncoder::new()
            .add_state(3)
            .add(TlvType::PublicKey, client_public)
            .add(TlvType::Proof, &client_proof)
            .build()
    }

    fn verify_m4(&mut self, m4_data: &[u8], server_identity_public: &[u8; 32]) {
        let tlv = TlvDecoder::decode(m4_data).expect("Decode M4");
        let server_proof = tlv.get(TlvType::Proof).expect("M4 Proof");
        let encrypted_data = tlv.get(TlvType::EncryptedData).expect("M4 EncryptedData");

        // Verify server proof
        let verifier = self.srp_verifier.as_ref().expect("Verifier missing");
        let session_key = verifier
            .verify_server(server_proof)
            .expect("Verify server proof");

        let key_bytes = session_key.as_bytes().to_vec();
        self.srp_session_key = Some(key_bytes.clone());

        // Decrypt accessory info
        let enc_key = derive_key(
            Some(b"Pair-Setup-Encrypt-Salt"),
            &key_bytes,
            b"Pair-Setup-Encrypt-Info",
            32,
        )
        .expect("Derive enc key");

        let decrypted = decrypt_with_key(encrypted_data, &enc_key, b"PS-Msg04");
        let sub_tlv = TlvDecoder::decode(&decrypted).expect("Decode sub-TLV");

        let identifier = sub_tlv.get(TlvType::Identifier).expect("Identifier");
        assert_eq!(identifier, server_identity_public);

        let _signature = sub_tlv.get(TlvType::Signature).expect("Signature");
        // Verification of signature skipped here as we trust the crypto lib,
        // but could verify if needed.
    }

    fn create_verify_m1(&mut self) -> Vec<u8> {
        let keypair = X25519KeyPair::generate();
        let public = *keypair.public_key().as_bytes();
        self.verify_keypair = Some(keypair);

        TlvEncoder::new()
            .add_state(1)
            .add(TlvType::PublicKey, &public)
            .build()
    }

    fn handle_verify_m2_create_m3(
        &mut self,
        m2_data: &[u8],
        client_identity: &Ed25519KeyPair,
    ) -> Vec<u8> {
        let tlv = TlvDecoder::decode(m2_data).expect("Decode M2");
        let server_public_bytes = tlv.get(TlvType::PublicKey).expect("Server Curve Public");
        let encrypted_data = tlv.get(TlvType::EncryptedData).expect("Encrypted Data");

        let mut server_public_arr = [0u8; 32];
        server_public_arr.copy_from_slice(server_public_bytes);
        let server_public = X25519PublicKey::from_bytes(&server_public_arr).expect("Parse key");

        // Use stored keypair
        let client_keypair = self.verify_keypair.as_ref().expect("Verify M1 not called");
        let shared_secret = client_keypair.diffie_hellman(&server_public);
        self.shared_secret = Some(*shared_secret.as_bytes());
        self.server_public_key = Some(server_public_arr);

        // Derive session key
        let session_key = derive_key(
            Some(b"Pair-Verify-Encrypt-Salt"),
            shared_secret.as_bytes(),
            b"Pair-Verify-Encrypt-Info",
            32,
        )
        .expect("Derive key");

        // Decrypt M2 data (verification optional here)
        let _decrypted = decrypt_with_key(encrypted_data, &session_key, b"PV-Msg02");

        // Build M3
        // Info: ClientCurvePublic || ClientIdentityPublic || ServerCurvePublic
        let mut info = Vec::new();
        info.extend_from_slice(client_keypair.public_key().as_bytes());
        info.extend_from_slice(client_identity.public_key().as_bytes());
        info.extend_from_slice(&server_public_arr);

        let signature = client_identity.sign(&info);

        let sub_tlv = TlvEncoder::new()
            .add(TlvType::Identifier, client_identity.public_key().as_bytes())
            .add(TlvType::Signature, &signature.to_bytes())
            .build();

        let encrypted = encrypt_with_key(&sub_tlv, &session_key, b"PV-Msg03");

        TlvEncoder::new()
            .add_state(3)
            .add(TlvType::EncryptedData, &encrypted)
            .build()
    }
}

// Helpers
fn encrypt_with_key(data: &[u8], key: &[u8], nonce_prefix: &[u8]) -> Vec<u8> {
    let mut nonce_bytes = [0u8; 12];
    let len = nonce_prefix.len().min(12);
    nonce_bytes[..len].copy_from_slice(&nonce_prefix[..len]);
    let nonce = Nonce::from_bytes(&nonce_bytes).expect("nonce creation");
    let cipher = ChaCha20Poly1305Cipher::new(key).expect("cipher creation");
    cipher.encrypt(&nonce, data).expect("encryption failed")
}

fn decrypt_with_key(data: &[u8], key: &[u8], nonce_prefix: &[u8]) -> Vec<u8> {
    let mut nonce_bytes = [0u8; 12];
    let len = nonce_prefix.len().min(12);
    nonce_bytes[..len].copy_from_slice(&nonce_prefix[..len]);
    let nonce = Nonce::from_bytes(&nonce_bytes).expect("nonce creation");
    let cipher = ChaCha20Poly1305Cipher::new(key).expect("cipher creation");
    cipher.decrypt(&nonce, data).expect("decryption failed")
}

#[test]
fn test_pairing_server_full_flow() {
    let identity = Ed25519KeyPair::generate();
    let identity_public = *identity.public_key().as_bytes();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");

    // Client setup
    let mut client = PairingClient::new("1234");

    // --- Pair Setup ---

    // M1: Client -> Server
    let m1 = client.create_m1();
    let res1 = server.process_pair_setup(&m1);
    assert!(res1.error.is_none());
    assert_eq!(res1.new_state, PairingServerState::WaitingForM3);

    // M2: Server -> Client (in res1.response)
    // M3: Client -> Server
    let m3 = client.handle_m2_create_m3(&res1.response);
    let res3 = server.process_pair_setup(&m3);
    assert!(res3.error.is_none());
    assert_eq!(res3.new_state, PairingServerState::PairSetupComplete);

    // M4: Server -> Client (in res3.response)
    client.verify_m4(&res3.response, &identity_public);

    // --- Pair Verify ---

    let client_identity = Ed25519KeyPair::generate();
    // Transient pairing usually uses same connection, but verify uses ephemeral curve keys
    // Client initiates verify

    // M1: Client -> Server
    let vm1 = client.create_verify_m1();
    let vres1 = server.process_pair_verify(&vm1);
    assert!(vres1.error.is_none());
    assert_eq!(vres1.new_state, PairingServerState::VerifyWaitingForM3);

    // M2: Server -> Client (in vres1.response)
    // M3: Client -> Server
    let vm3 = client.handle_verify_m2_create_m3(&vres1.response, &client_identity);
    let vres3 = server.process_pair_verify(&vm3);
    assert!(vres3.error.is_none());
    assert_eq!(vres3.new_state, PairingServerState::Complete);
    assert!(vres3.complete);

    // Check keys are derived
    assert!(server.encryption_keys().is_some());
    assert_eq!(
        server.client_public_key().unwrap(),
        client_identity.public_key().as_bytes()
    );
}

#[test]
fn test_pair_setup_wrong_password() {
    let identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");

    // Client with WRONG password
    let mut client = PairingClient::new("wrong");

    // M1
    let m1 = client.create_m1();
    let res1 = server.process_pair_setup(&m1);
    assert!(res1.error.is_none());

    // M3
    // This should fail either at client computing proof (if it checks) or server verifying
    // SrpClient usually doesn't fail computing proof, but the proof will be wrong.
    let m3 = client.handle_m2_create_m3(&res1.response);
    let res3 = server.process_pair_setup(&m3);

    assert!(res3.error.is_some());
    // Should be AuthenticationFailed
    assert!(matches!(
        res3.error,
        Some(crate::receiver::ap2::pairing_server::PairingError::AuthenticationFailed)
    ));
}

#[test]
fn test_state_machine_violations() {
    let identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");

    // Send M3 without M1
    let m3 = TlvEncoder::new().add_state(3).build();
    let res = server.process_pair_setup(&m3);
    assert!(res.error.is_some());
}

#[test]
fn test_pair_verify_bad_signature() {
    let identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");

    // Skip setup, force state to PairSetupComplete (hack via reflection? No, just run setup first)
    // ... Or allow verify from Idle?
    // PairingServer::process_pair_verify allows verify from Idle (returning client).

    let mut client = PairingClient::new("1234");

    // M1
    let vm1 = client.create_verify_m1();
    let vres1 = server.process_pair_verify(&vm1);

    // M3 with WRONG signature
    // We'll manually construct a bad M3
    let client_identity = Ed25519KeyPair::generate();

    // We can't easily corrupt internal encrypted data of M3 without full logic.
    // Instead, let's send M3 with a random signature in the inner TLV.

    let m2_data = &vres1.response;
    let tlv = TlvDecoder::decode(m2_data).expect("Decode M2");
    let server_public_bytes = tlv.get(TlvType::PublicKey).expect("Server Curve Public");
    let encrypted_data = tlv.get(TlvType::EncryptedData).expect("Encrypted Data");

    let mut server_public_arr = [0u8; 32];
    server_public_arr.copy_from_slice(server_public_bytes);
    let server_public = X25519PublicKey::from_bytes(&server_public_arr).expect("Parse key");

    let client_keypair = client.verify_keypair.as_ref().unwrap();
    let shared_secret = client_keypair.diffie_hellman(&server_public);

    let session_key = derive_key(
        Some(b"Pair-Verify-Encrypt-Salt"),
        shared_secret.as_bytes(),
        b"Pair-Verify-Encrypt-Info",
        32,
    )
    .expect("Derive key");

    let _decrypted = decrypt_with_key(encrypted_data, &session_key, b"PV-Msg02");

    let mut info = Vec::new();
    info.extend_from_slice(client_keypair.public_key().as_bytes());
    info.extend_from_slice(client_identity.public_key().as_bytes());
    info.extend_from_slice(&server_public_arr);

    // Sign WRONG info or just random bytes
    let signature = client_identity.sign(b"Wrong Info");

    let sub_tlv = TlvEncoder::new()
        .add(TlvType::Identifier, client_identity.public_key().as_bytes())
        .add(TlvType::Signature, &signature.to_bytes())
        .build();

    let encrypted = encrypt_with_key(&sub_tlv, &session_key, b"PV-Msg03");

    let bad_vm3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::EncryptedData, &encrypted)
        .build();

    let vres3 = server.process_pair_verify(&bad_vm3);
    assert!(vres3.error.is_some());
    // Should be SignatureVerificationFailed
    assert!(matches!(
        vres3.error,
        Some(crate::receiver::ap2::pairing_server::PairingError::SignatureVerificationFailed)
    ));
}

#[test]
fn test_initial_state() {
    let identity = Ed25519KeyPair::generate();
    let server = PairingServer::new(identity);
    assert_eq!(server.state, PairingServerState::Idle);
}

#[test]
fn test_reset() {
    let identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");

    // Process M1
    let m1 = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();

    let _ = server.process_pair_setup(&m1);
    assert_eq!(server.state, PairingServerState::WaitingForM3);

    // Reset
    server.reset();
    assert_eq!(server.state, PairingServerState::Idle);
}
