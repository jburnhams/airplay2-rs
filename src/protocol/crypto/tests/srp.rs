use super::super::*;
use rand::RngCore;

#[test]
fn test_srp_client_creation() {
    let client = SrpClient::new().unwrap();
    assert!(!client.public_key().is_empty());
}

// Note: This test is ignored because our SRP implementation uses the HomeKit/AirPlay 2
// M1 calculation (M1 = H(H(N) ^ H(g), H(username), salt, A, B, K)) which differs from
// the standard RFC 5054 implementation used by the `srp` crate. These are incompatible.
// The actual pairing flow is tested in integration tests.
#[test]
#[ignore]
fn test_srp_handshake() {
    // 1. Client setup
    let client = SrpClient::new().unwrap();
    let username = b"Pair-Setup";
    let password = b"1234";
    let client_a = client.public_key();

    // 2. Server setup (simulation)
    let salt = b"randomsalt";

    // Use Client to compute verifier (simulating registration)
    let helper_client = ::srp::client::SrpClient::<sha2::Sha512>::new(&::srp::groups::G_3072);
    let verifier = helper_client.compute_verifier(username, password, salt);

    let server = ::srp::server::SrpServer::<sha2::Sha512>::new(&::srp::groups::G_3072);

    // Server generates ephemeral B
    let mut b_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b_bytes);
    let server_b_pub = server.compute_public_ephemeral(&b_bytes, &verifier);

    // 3. Client processes challenge
    let client_verifier = client
        .process_challenge(username, password, salt, &server_b_pub)
        .expect("Client failed to process challenge");

    // 4. Client generates proof
    let client_m1 = client_verifier.client_proof();

    // 5. Server verifies client
    let server_verifier = server
        .process_reply(&b_bytes, &verifier, client_a)
        .expect("Server failed to process reply");

    server_verifier
        .verify_client(client_m1)
        .expect("Server failed to verify client");
    let server_key = server_verifier.key();

    let server_m2 = server_verifier.proof();

    // 6. Client verifies server
    let client_key = client_verifier
        .verify_server(server_m2)
        .expect("Client failed to verify server");

    assert_eq!(client_key.as_bytes(), server_key);
}

#[test]
fn test_srp_invalid_password_fails() {
    let client = SrpClient::new().unwrap();
    let username = b"Pair-Setup";
    let password = b"correct";
    let salt = b"salt";

    // Helper for registration
    let helper_client = ::srp::client::SrpClient::<sha2::Sha512>::new(&::srp::groups::G_3072);
    // Server registered with "wrong" password
    let verifier = helper_client.compute_verifier(username, b"wrong", salt);

    let server = ::srp::server::SrpServer::<sha2::Sha512>::new(&::srp::groups::G_3072);
    let mut b_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b_bytes);
    let server_b_pub = server.compute_public_ephemeral(&b_bytes, &verifier);

    // Client tries with "correct" password
    let client_verifier = client
        .process_challenge(username, password, salt, &server_b_pub)
        .unwrap();

    let client_m1 = client_verifier.client_proof();

    let server_verifier = server
        .process_reply(&b_bytes, &verifier, client.public_key())
        .unwrap();

    // Verification should fail
    assert!(server_verifier.verify_client(client_m1).is_err());
}
