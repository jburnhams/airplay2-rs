//! Integration tests for HomeKit pairing
//!
//! These tests simulate a complete pairing flow between a mock
//! client and our pairing server.

use airplay2::protocol::crypto::{Ed25519KeyPair, SrpClient, SrpParams, X25519KeyPair};
use airplay2::protocol::pairing::tlv::{TlvDecoder, TlvEncoder, TlvType};
use airplay2::receiver::ap2::pairing_server::{PairingServer, PairingServerState};

/// Test complete pair-setup flow
#[test]
fn test_complete_pair_setup() {
    // Create server
    let server_identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(server_identity);
    server.set_password("1234");

    // Create client
    let client = SrpClient::new(&SrpParams::RFC5054_3072).expect("Failed to create client");

    // M1: Client initiates
    let m1 = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();

    let m2_result = server.process_pair_setup(&m1);
    assert!(m2_result.error.is_none());

    // Parse M2
    let m2_tlv = TlvDecoder::decode(&m2_result.response).unwrap();
    let salt = m2_tlv.get(TlvType::Salt).unwrap();
    let server_public = m2_tlv.get(TlvType::PublicKey).unwrap();

    // Client computes proof
    let client_verifier = client
        .process_challenge(b"Pair-Setup", b"1234", salt, server_public)
        .expect("Client should process challenge");

    let client_public = client.public_key();
    let client_proof = client_verifier.client_proof();

    // M3: Client sends proof
    let m3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::PublicKey, client_public)
        .add(TlvType::Proof, client_proof)
        .build();

    let m4_result = server.process_pair_setup(&m3);

    // Should succeed if password matches
    if let Some(e) = &m4_result.error {
        panic!("M4 error: {:?}", e);
    }

    assert!(m4_result.error.is_none());
    assert_eq!(m4_result.new_state, PairingServerState::PairSetupComplete);

    // Verify server proof
    let m4_tlv = TlvDecoder::decode(&m4_result.response).unwrap();
    let server_proof = m4_tlv.get(TlvType::Proof).unwrap();

    let _session_key = client_verifier
        .verify_server(server_proof)
        .expect("Server proof verification failed");
}

/// Test wrong password rejection
#[test]
fn test_wrong_password_rejected() {
    let server_identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(server_identity);
    server.set_password("1234");

    // Client with wrong password
    let client = SrpClient::new(&SrpParams::RFC5054_3072).expect("Failed to create client");

    // M1
    let m1 = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();

    let m2_result = server.process_pair_setup(&m1);
    let m2_tlv = TlvDecoder::decode(&m2_result.response).unwrap();
    let salt = m2_tlv.get(TlvType::Salt).unwrap();
    let server_public = m2_tlv.get(TlvType::PublicKey).unwrap();

    // Client computes (wrong) proof
    let client_verifier = client
        .process_challenge(b"Pair-Setup", b"0000", salt, server_public)
        .expect("Client should process challenge");

    let client_public = client.public_key();
    let client_proof = client_verifier.client_proof();

    // M3 with wrong proof
    let m3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::PublicKey, client_public)
        .add(TlvType::Proof, client_proof)
        .build();

    let m4_result = server.process_pair_setup(&m3);

    // Should fail authentication
    assert!(m4_result.error.is_some());
}

/// Test pair-verify after successful pair-setup
#[test]
fn test_pair_verify_after_setup() {
    // This test requires a complete pair-setup first
    // For brevity, we test pair-verify in isolation
    // by manually setting up the required state

    let server_identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(server_identity);

    // Simulate completed pair-setup by setting state
    // In production, this would follow actual pair-setup
    // But `process_pair_verify` resets if not in PairSetupComplete or Idle.
    // So we can start from Idle (new connection scenario).
    // server.state is private, so we can't set it directly.
    // We rely on `process_pair_verify` resetting if in Idle.

    let client_keypair = X25519KeyPair::generate();

    let m1 = TlvEncoder::new()
        .add_state(1)
        .add(TlvType::PublicKey, client_keypair.public_key().as_bytes())
        .build();

    let m2_result = server.process_pair_verify(&m1);

    // Should get M2 response with server's public key
    if let Some(e) = &m2_result.error {
        panic!("M2 verify error: {:?}", e);
    }
    assert!(m2_result.error.is_none());

    let m2_tlv = TlvDecoder::decode(&m2_result.response).unwrap();
    assert_eq!(m2_tlv.get_state().ok(), Some(2));
    assert!(m2_tlv.get(TlvType::PublicKey).is_some());
    assert!(m2_tlv.get(TlvType::EncryptedData).is_some());
}
