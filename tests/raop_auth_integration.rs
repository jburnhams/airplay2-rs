// tests/raop_auth_integration.rs

use airplay2::protocol::crypto::RaopRsaPrivateKey;
use airplay2::protocol::raop::{build_response_message, generate_challenge, generate_response};

#[test]
fn test_simulated_airplay1_auth() {
    // Simulate client-server authentication

    // Server has private key
    let server_key = RaopRsaPrivateKey::generate().unwrap();
    let server_ip: std::net::IpAddr = "192.168.1.50".parse().unwrap();
    let server_mac = [0x00, 0x50, 0xC2, 0x12, 0xA2, 0x3F];

    // Client generates challenge
    let challenge = generate_challenge();

    // Server generates response
    let response = generate_response(&server_key, &challenge, &server_ip, &server_mac).unwrap();

    // In real scenario, client would verify with Apple's public key
    // Here we verify with the test server's public key
    let public = server_key.public_key();

    use base64::Engine;
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;
    use sha1::Sha1;

    let sig_bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(&response)
        .unwrap();
    let verifying_key = VerifyingKey::<Sha1>::new(public);
    let sig = Signature::try_from(sig_bytes.as_slice()).unwrap();

    let message = build_response_message(&challenge, &server_ip, &server_mac);

    verifying_key.verify(&message, &sig).unwrap();
}
