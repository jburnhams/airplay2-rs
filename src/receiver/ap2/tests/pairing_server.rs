use crate::protocol::crypto::{
    ChaCha20Poly1305Cipher, Ed25519KeyPair, HkdfSha512, Nonce, SrpClient, X25519KeyPair,
    X25519PublicKey,
};
use crate::protocol::pairing::tlv::{TlvDecoder, TlvEncoder, TlvType};
use crate::receiver::ap2::pairing_server::{PairingServer, PairingServerState};

fn create_test_server() -> PairingServer {
    let identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");
    server
}

#[test]
fn test_m1_handling() {
    let mut server = create_test_server();

    // Build M1 TLV
    let m1 = TlvEncoder::new()
        .add_u8(TlvType::State, 1)
        .add_u8(TlvType::Method, 0)
        .encode();

    let result = server.process_pair_setup(&m1);

    assert!(result.error.is_none());
    assert_eq!(result.new_state, PairingServerState::WaitingForM3);

    // Response should contain state=2, salt, and public key
    let response_tlv = TlvDecoder::decode(&result.response).unwrap();
    assert_eq!(response_tlv.get_u8(TlvType::State), Some(2));
    assert!(response_tlv.get_bytes(TlvType::Salt).is_some());
    assert!(response_tlv.get_bytes(TlvType::PublicKey).is_some());
}

#[test]
fn test_state_machine_enforcement() {
    let mut server = create_test_server();

    // Try M3 before M1 - should fail
    let m3 = TlvEncoder::new().add_u8(TlvType::State, 3).encode();

    let result = server.process_pair_setup(&m3);
    assert!(result.error.is_some());
}

#[test]
fn test_reset() {
    let mut server = create_test_server();

    // Process M1
    let m1 = TlvEncoder::new()
        .add_u8(TlvType::State, 1)
        .add_u8(TlvType::Method, 0)
        .encode();

    let result = server.process_pair_setup(&m1);
    assert_eq!(result.new_state, PairingServerState::WaitingForM3);

    // Reset
    server.reset();

    // Process M1 again - should work if reset to Idle
    let result = server.process_pair_setup(&m1);
    assert_eq!(result.new_state, PairingServerState::WaitingForM3);
}

#[test]
fn test_complete_pair_setup() {
    // Create server
    let server_identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(server_identity);
    server.set_password("1234");

    // Create client
    // Note: SrpClient::new() uses default params which match server
    let client = SrpClient::new().unwrap();

    // M1: Client initiates
    let m1 = TlvEncoder::new()
        .add_u8(TlvType::State, 1)
        .add_u8(TlvType::Method, 0)
        .encode();

    let m2_result = server.process_pair_setup(&m1);
    assert!(m2_result.error.is_none());

    // Parse M2
    let m2_tlv = TlvDecoder::decode(&m2_result.response).unwrap();
    let salt = m2_tlv.get_bytes(TlvType::Salt).unwrap();
    let server_public = m2_tlv.get_bytes(TlvType::PublicKey).unwrap();

    // Client computes proof
    let verifier = client
        .process_challenge(b"Pair-Setup", b"1234", salt, server_public)
        .expect("Client should process challenge");

    // M3: Client sends proof
    let m3 = TlvEncoder::new()
        .add_u8(TlvType::State, 3)
        .add_bytes(TlvType::PublicKey, client.public_key())
        .add_bytes(TlvType::Proof, verifier.client_proof())
        .encode();

    let m4_result = server.process_pair_setup(&m3);

    assert!(m4_result.error.is_none());
    assert_eq!(m4_result.new_state, PairingServerState::PairSetupComplete);
}

#[test]
fn test_pair_verify() {
    // 1. Setup Server
    let server_identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(server_identity);

    // 2. Client Setup
    let client_x25519 = X25519KeyPair::generate();
    let client_identity = Ed25519KeyPair::generate();

    // 3. Client sends M1
    let m1 = TlvEncoder::new()
        .add_u8(TlvType::State, 1)
        .add_bytes(TlvType::PublicKey, client_x25519.public_key().as_bytes())
        .encode();

    // 4. Server processes M1
    let result_m1 = server.process_pair_verify(&m1);
    assert!(result_m1.error.is_none());
    assert_eq!(result_m1.new_state, PairingServerState::VerifyWaitingForM3);

    // 5. Parse M2
    let m2_tlv = TlvDecoder::decode(&result_m1.response).unwrap();
    let server_x25519_bytes = m2_tlv.get_bytes(TlvType::PublicKey).unwrap();
    let _encrypted_m2 = m2_tlv.get_bytes(TlvType::EncryptedData).unwrap();

    let server_x25519 = X25519PublicKey::from_bytes(server_x25519_bytes).unwrap();

    // 6. Client derives shared secret and session key
    let shared_secret = client_x25519.diffie_hellman(&server_x25519);
    let hkdf = HkdfSha512::new(Some(b"Pair-Verify-Encrypt-Salt"), shared_secret.as_bytes());
    let session_key = hkdf
        .expand_fixed::<32>(b"Pair-Verify-Encrypt-Info")
        .unwrap();

    // 7. Client prepares M3 (Encrypted Signature)
    // Accessory Info: ClientX25519 || ClientEd25519 || ServerX25519
    let mut info = Vec::new();
    info.extend_from_slice(client_x25519.public_key().as_bytes());
    info.extend_from_slice(client_identity.public_key().as_bytes());
    info.extend_from_slice(server_x25519.as_bytes());

    let signature = client_identity.sign(&info);

    let sub_tlv = TlvEncoder::new()
        .add_bytes(TlvType::Identifier, client_identity.public_key().as_bytes())
        .add_bytes(TlvType::Signature, &signature.to_bytes())
        .encode();

    // Encrypt M3
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[..8].copy_from_slice(b"PV-Msg03");
    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
    let cipher = ChaCha20Poly1305Cipher::new(&session_key).unwrap();
    let encrypted_m3 = cipher.encrypt(&nonce, &sub_tlv).unwrap();

    let m3 = TlvEncoder::new()
        .add_u8(TlvType::State, 3)
        .add_bytes(TlvType::EncryptedData, &encrypted_m3)
        .encode();

    // 9. Server processes M3
    let result_m3 = server.process_pair_verify(&m3);

    if let Some(e) = &result_m3.error {
        println!("Error: {e:?}");
    }
    assert!(result_m3.error.is_none());
    assert_eq!(result_m3.new_state, PairingServerState::Complete);
    assert!(result_m3.complete);
}
