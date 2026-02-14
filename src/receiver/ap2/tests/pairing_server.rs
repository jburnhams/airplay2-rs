use crate::protocol::crypto::{
    ChaCha20Poly1305Cipher, Nonce, X25519KeyPair, X25519PublicKey, derive_key,
};
use crate::protocol::crypto::{Ed25519KeyPair, SrpClient, SrpParams};
use crate::protocol::pairing::tlv::{TlvDecoder, TlvEncoder, TlvType};
use crate::receiver::ap2::pairing_server::{PairingServer, PairingServerState};

fn create_test_server() -> PairingServer {
    let identity = Ed25519KeyPair::generate();
    let mut server = PairingServer::new(identity);
    server.set_password("1234");
    server
}

#[test]
fn test_initial_state() {
    let server = create_test_server();
    assert_eq!(server.state, PairingServerState::Idle);
}

#[test]
fn test_m1_handling() {
    let mut server = create_test_server();

    // Build M1 TLV
    let m1 = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();

    let result = server.process_pair_setup(&m1);

    assert!(result.error.is_none());
    assert_eq!(result.new_state, PairingServerState::WaitingForM3);

    // Response should contain state=2, salt, and public key
    let response_tlv = TlvDecoder::decode(&result.response).unwrap();
    assert_eq!(response_tlv.get_state().ok(), Some(2));
    assert!(response_tlv.get(TlvType::Salt).is_some());
    assert!(response_tlv.get(TlvType::PublicKey).is_some());
}

#[test]
fn test_state_machine_enforcement() {
    let mut server = create_test_server();

    // Try M3 before M1 - should fail
    let m3 = TlvEncoder::new().add_state(3).build();

    let result = server.process_pair_setup(&m3);
    assert!(result.error.is_some());
}

#[test]
fn test_reset() {
    let mut server = create_test_server();

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

#[test]
fn test_full_pair_setup_flow() {
    let mut server = create_test_server();

    // 1. Client sends M1
    let m1 = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();

    let result_m1 = server.process_pair_setup(&m1);
    assert!(result_m1.error.is_none());
    let m2_tlv = TlvDecoder::decode(&result_m1.response).unwrap();
    let salt = m2_tlv.get(TlvType::Salt).unwrap();
    let server_public = m2_tlv.get(TlvType::PublicKey).unwrap();

    // 2. Client computes M3
    let srp_client = SrpClient::new(&SrpParams::RFC5054_3072).unwrap();

    let client_public = srp_client.public_key();
    let verifier = srp_client
        .process_challenge(b"Pair-Setup", b"1234", salt, server_public)
        .unwrap();
    let client_proof = verifier.client_proof();

    let m3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::PublicKey, client_public)
        .add(TlvType::Proof, client_proof)
        .build();

    // 3. Server processes M3
    let result_m3 = server.process_pair_setup(&m3);
    assert!(result_m3.error.is_none());
    assert_eq!(result_m3.new_state, PairingServerState::PairSetupComplete);

    // Verify M4 response
    let m4_tlv = TlvDecoder::decode(&result_m3.response).unwrap();
    assert_eq!(m4_tlv.get_state().ok(), Some(4));

    // Verify server proof (client side verification)
    let server_proof = m4_tlv.get(TlvType::Proof).unwrap();
    assert!(verifier.verify_server(server_proof).is_ok());

    // Verify EncryptedData present
    assert!(m4_tlv.get(TlvType::EncryptedData).is_some());
}

#[test]
fn test_pair_setup_wrong_password() {
    let mut server = create_test_server();
    // Server expects "1234"

    // 1. M1
    let m1 = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();
    let result_m1 = server.process_pair_setup(&m1);
    let m2_tlv = TlvDecoder::decode(&result_m1.response).unwrap();
    let salt = m2_tlv.get(TlvType::Salt).unwrap();
    let server_public = m2_tlv.get(TlvType::PublicKey).unwrap();

    // 2. Client uses wrong password
    let srp_client = SrpClient::new(&SrpParams::RFC5054_3072).unwrap();
    let client_public = srp_client.public_key();
    let verifier = srp_client
        .process_challenge(b"Pair-Setup", b"wrong_pass", salt, server_public)
        .unwrap();
    let client_proof = verifier.client_proof();

    let m3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::PublicKey, client_public)
        .add(TlvType::Proof, client_proof)
        .build();

    // 3. Server should reject M3
    let result_m3 = server.process_pair_setup(&m3);
    assert!(result_m3.error.is_some());
    // Error code 2 is AuthenticationFailed
    let err_tlv = TlvDecoder::decode(&result_m3.response).unwrap();
    assert_eq!(
        err_tlv.get(TlvType::Error).and_then(|v| v.first()).copied(),
        Some(2)
    );
}

#[test]
fn test_pair_verify_flow() {
    let mut server = create_test_server();

    // --- Run Setup ---
    let m1 = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();
    let res_m1 = server.process_pair_setup(&m1);
    let m2_tlv = TlvDecoder::decode(&res_m1.response).unwrap();
    let salt = m2_tlv.get(TlvType::Salt).unwrap();
    let server_pub = m2_tlv.get(TlvType::PublicKey).unwrap();

    let srp_client = SrpClient::new(&SrpParams::RFC5054_3072).unwrap();
    let client_srp_pub = srp_client.public_key();
    let verifier = srp_client
        .process_challenge(b"Pair-Setup", b"1234", salt, server_pub)
        .unwrap();
    let client_proof = verifier.client_proof();

    let m3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::PublicKey, client_srp_pub)
        .add(TlvType::Proof, client_proof)
        .build();
    let _ = server.process_pair_setup(&m3);

    // --- Run Verify ---
    // 1. Client M1
    let client_curve_kp = X25519KeyPair::generate();

    let vm1 = TlvEncoder::new()
        .add_state(1)
        .add(TlvType::PublicKey, client_curve_kp.public_key().as_bytes())
        .build();

    let verify_res_m1 = server.process_pair_verify(&vm1);
    assert!(verify_res_m1.error.is_none());
    assert_eq!(
        verify_res_m1.new_state,
        PairingServerState::VerifyWaitingForM3
    );

    let vm2_tlv = TlvDecoder::decode(&verify_res_m1.response).unwrap();
    let server_curve_pub_bytes = vm2_tlv.get(TlvType::PublicKey).unwrap();

    // Client derives session key
    let mut arr = [0u8; 32];
    arr.copy_from_slice(server_curve_pub_bytes);
    let server_curve_pub = X25519PublicKey::from_bytes(&arr).unwrap();
    let shared_secret = client_curve_kp.diffie_hellman(&server_curve_pub);

    let session_key = derive_key(
        Some(b"Pair-Verify-Encrypt-Salt"),
        shared_secret.as_bytes(),
        b"Pair-Verify-Encrypt-Info",
        32,
    )
    .unwrap();

    // 2. Client sends M3
    let client_identity = Ed25519KeyPair::generate();

    let mut info = Vec::new();
    info.extend_from_slice(client_curve_kp.public_key().as_bytes());
    info.extend_from_slice(client_identity.public_key().as_bytes());
    info.extend_from_slice(server_curve_pub.as_bytes());

    let signature = client_identity.sign(&info);

    let sub_tlv = TlvEncoder::new()
        .add(TlvType::Identifier, client_identity.public_key().as_bytes())
        .add(TlvType::Signature, &signature.to_bytes())
        .build();

    // Encrypt
    let mut nonce_bytes = [0u8; 12];
    let nonce_prefix = b"PV-Msg03";
    let len = nonce_prefix.len().min(12);
    nonce_bytes[..len].copy_from_slice(&nonce_prefix[..len]);
    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
    let cipher = ChaCha20Poly1305Cipher::new(&session_key).unwrap();
    let encrypted_vm3 = cipher.encrypt(&nonce, &sub_tlv).unwrap();

    let vm3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::EncryptedData, &encrypted_vm3)
        .build();

    let res_vm3 = server.process_pair_verify(&vm3);
    assert!(res_vm3.error.is_none());
    assert_eq!(res_vm3.new_state, PairingServerState::Complete);
    assert!(res_vm3.complete);
}

#[test]
fn test_pair_verify_bad_signature() {
    let mut server = create_test_server();
    // Setup first
    let m1 = TlvEncoder::new()
        .add_state(1)
        .add_byte(TlvType::Method, 0)
        .build();
    let res_m1 = server.process_pair_setup(&m1);
    let m2_tlv = TlvDecoder::decode(&res_m1.response).unwrap();
    let salt = m2_tlv.get(TlvType::Salt).unwrap();
    let server_pub = m2_tlv.get(TlvType::PublicKey).unwrap();

    let srp_client = SrpClient::new(&SrpParams::RFC5054_3072).unwrap();
    let client_srp_pub = srp_client.public_key();
    let verifier = srp_client
        .process_challenge(b"Pair-Setup", b"1234", salt, server_pub)
        .unwrap();
    let client_proof = verifier.client_proof();

    let m3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::PublicKey, client_srp_pub)
        .add(TlvType::Proof, client_proof)
        .build();
    let _ = server.process_pair_setup(&m3);

    // Verify M1
    let client_curve_kp = X25519KeyPair::generate();
    let vm1 = TlvEncoder::new()
        .add_state(1)
        .add(TlvType::PublicKey, client_curve_kp.public_key().as_bytes())
        .build();
    let verify_res_m1 = server.process_pair_verify(&vm1);
    let vm2_tlv = TlvDecoder::decode(&verify_res_m1.response).unwrap();
    let server_curve_pub_bytes = vm2_tlv.get(TlvType::PublicKey).unwrap();

    // Verify M3 with WRONG signature
    let mut arr = [0u8; 32];
    arr.copy_from_slice(server_curve_pub_bytes);
    let server_curve_pub = X25519PublicKey::from_bytes(&arr).unwrap();
    let shared_secret = client_curve_kp.diffie_hellman(&server_curve_pub);
    let session_key = derive_key(
        Some(b"Pair-Verify-Encrypt-Salt"),
        shared_secret.as_bytes(),
        b"Pair-Verify-Encrypt-Info",
        32,
    )
    .unwrap();

    let client_identity = Ed25519KeyPair::generate();
    // Sign WRONG data
    let signature = client_identity.sign(b"wrong data");

    let sub_tlv = TlvEncoder::new()
        .add(TlvType::Identifier, client_identity.public_key().as_bytes())
        .add(TlvType::Signature, &signature.to_bytes())
        .build();

    let mut nonce_bytes = [0u8; 12];
    let nonce_prefix = b"PV-Msg03";
    let len = nonce_prefix.len().min(12);
    nonce_bytes[..len].copy_from_slice(&nonce_prefix[..len]);
    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
    let cipher = ChaCha20Poly1305Cipher::new(&session_key).unwrap();
    let encrypted_vm3 = cipher.encrypt(&nonce, &sub_tlv).unwrap();

    let vm3 = TlvEncoder::new()
        .add_state(3)
        .add(TlvType::EncryptedData, &encrypted_vm3)
        .build();
    let res_vm3 = server.process_pair_verify(&vm3);

    assert!(res_vm3.error.is_some());
}
