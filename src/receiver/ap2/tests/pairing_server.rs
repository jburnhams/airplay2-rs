use crate::protocol::crypto::Ed25519KeyPair;
use crate::protocol::crypto::SrpClient;
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
