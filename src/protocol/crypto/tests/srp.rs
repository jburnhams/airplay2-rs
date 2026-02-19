use rand::RngCore;

use super::super::*;

#[test]
fn test_srp_client_creation() {
    let client = SrpClient::new(&SrpParams::RFC5054_3072).unwrap();
    assert!(!client.public_key().is_empty());
}

#[test]
fn test_srp_internal_handshake() {
    // 1. Setup parameters
    let username = b"Pair-Setup";
    let password = b"1234";
    let params = SrpParams::RFC5054_3072;

    // 2. Client setup
    let client = SrpClient::new(&params).unwrap();
    let client_a = client.public_key();

    // 3. Server setup
    let salt = b"randomsalt";
    // Compute verifier using internal implementation
    let verifier = SrpServer::compute_verifier(username, password, salt, &params);
    let server = SrpServer::new(&verifier, &params);
    let server_b = server.public_key();

    // 4. Client processes challenge
    let client_verifier = client
        .process_challenge(username, password, salt, server_b)
        .expect("Client failed to process challenge");

    // 5. Client generates proof (M1)
    let client_m1 = client_verifier.client_proof();

    // 6. Server verifies client and generates proof (M2)
    let (server_key, server_m2) = server
        .verify_client(username, salt, client_a, client_m1)
        .expect("Server failed to verify client");

    // 7. Client verifies server
    let client_key = client_verifier
        .verify_server(&server_m2)
        .expect("Client failed to verify server");

    // 8. Verify shared keys match
    assert_eq!(client_key.as_bytes(), server_key.as_bytes());
}

#[test]
fn test_srp_invalid_password_fails() {
    let username = b"Pair-Setup";
    let password = b"correct";
    let params = SrpParams::RFC5054_3072;
    let client = SrpClient::new(&params).unwrap();
    let salt = b"salt";

    // Server registered with "wrong" password
    let verifier = SrpServer::compute_verifier(username, b"wrong", salt, &params);
    let server = SrpServer::new(&verifier, &params);
    let server_b = server.public_key();

    // Client tries with "correct" password
    let client_verifier = client
        .process_challenge(username, password, salt, server_b)
        .unwrap();

    let client_m1 = client_verifier.client_proof();

    // Verification should fail
    assert!(
        server
            .verify_client(username, salt, client.public_key(), client_m1)
            .is_err()
    );
}
