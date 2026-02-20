use rand::RngCore;

use super::super::*;

#[test]
fn test_srp_client_creation() {
    let client = SrpClient::new(&SrpParams::RFC5054_3072).unwrap();
    assert!(!client.public_key().is_empty());
}

#[test]
fn test_srp_handshake() {
    // 1. Setup parameters
    let username = b"Pair-Setup";
    let password = b"1234";
    let salt = b"randomsalt"; // In practice, random bytes
    let params = SrpParams::RFC5054_3072;

    // 2. Server registration (compute verifier)
    // This happens once when pairing is established
    let verifier = SrpServer::compute_verifier(username, password, salt, &params);

    // 3. Server session initialization
    // Server creates a session using the stored verifier
    let server = SrpServer::new(&verifier, &params);
    let server_b_pub = server.public_key();

    // 4. Client session initialization
    let client = SrpClient::new(&params).unwrap();
    let client_a_pub = client.public_key();

    // 5. Client processes challenge (M1 generation)
    let client_verifier = client
        .process_challenge(username, password, salt, server_b_pub)
        .expect("Client failed to process challenge");

    let client_proof_m1 = client_verifier.client_proof();

    // 6. Server verifies client (M2 generation)
    let (server_key, server_proof_m2) = server
        .verify_client(username, salt, client_a_pub, client_proof_m1)
        .expect("Server failed to verify client");

    // 7. Client verifies server
    let client_key = client_verifier
        .verify_server(&server_proof_m2)
        .expect("Client failed to verify server");

    // 8. Verify keys match
    assert_eq!(client_key.as_bytes(), server_key.as_bytes());
}

#[test]
fn test_srp_invalid_password_fails() {
    let username = b"Pair-Setup";
    let password_correct = b"correct";
    let password_wrong = b"wrong";
    let salt = b"salt";
    let params = SrpParams::RFC5054_3072;

    // Server registered with correct password
    let verifier = SrpServer::compute_verifier(username, password_correct, salt, &params);
    let server = SrpServer::new(&verifier, &params);
    let server_b_pub = server.public_key();

    // Client tries with WRONG password
    let client = SrpClient::new(&params).unwrap();
    let client_verifier = client
        .process_challenge(username, password_wrong, salt, server_b_pub)
        .unwrap();

    let client_proof_m1 = client_verifier.client_proof();

    // Server verification should fail
    let result = server.verify_client(username, salt, client.public_key(), client_proof_m1);

    assert!(result.is_err(), "Server should reject invalid password");
}
