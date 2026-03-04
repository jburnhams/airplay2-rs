use rand::RngCore;

use super::super::*;

#[test]
fn test_srp_client_creation() {
    let client = SrpClient::new(&SrpParams::RFC5054_3072).unwrap();
    assert!(!client.public_key().is_empty());
}

#[test]
fn test_srp_handshake() {
    // 1. Client setup
    let username = b"Pair-Setup";
    let password = b"1234";
    let client = SrpClient::new(&SrpParams::RFC5054_3072).unwrap();
    let client_a = client.public_key();

    // 2. Server setup (simulation)
    let salt = b"randomsalt";

    // Use Server to compute verifier (simulating registration)
    let verifier = SrpServer::compute_verifier(username, password, salt, &SrpParams::RFC5054_3072);

    let server = SrpServer::new(&verifier, &SrpParams::RFC5054_3072);

    // Server generates ephemeral B
    let server_b_pub = server.public_key();

    // 3. Client processes challenge
    let client_verifier = client
        .process_challenge(username, password, salt, server_b_pub)
        .expect("Client failed to process challenge");

    // 4. Client generates proof
    let client_m1 = client_verifier.client_proof();

    // 5. Server verifies client
    let (server_session, server_m2) = server
        .verify_client(username, salt, client_a, client_m1)
        .expect("Server failed to verify client");
    let server_key = server_session.as_bytes();

    // 6. Client verifies server
    let client_key = client_verifier
        .verify_server(&server_m2)
        .expect("Client failed to verify server");

    assert_eq!(client_key.as_bytes(), server_key);
}

#[test]
fn test_srp_invalid_password_fails() {
    let username = b"Pair-Setup";
    let password = b"correct";
    let client = SrpClient::new(&SrpParams::RFC5054_3072).unwrap();
    let salt = b"salt";

    // Helper for registration
    // Server registered with "wrong" password
    let verifier = SrpServer::compute_verifier(username, b"wrong", salt, &SrpParams::RFC5054_3072);

    let server = SrpServer::new(&verifier, &SrpParams::RFC5054_3072);
    let server_b_pub = server.public_key();

    // Client tries with "correct" password
    let client_verifier = client
        .process_challenge(username, password, salt, server_b_pub)
        .unwrap();

    let client_m1 = client_verifier.client_proof();

    // Verification should fail
    assert!(
        server
            .verify_client(username, salt, client.public_key(), client_m1)
            .is_err()
    );
}
