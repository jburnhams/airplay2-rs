use super::super::*;

#[test]
fn test_ed25519_keypair_generation() {
    let kp = Ed25519KeyPair::generate();
    let pk = kp.public_key();

    assert_eq!(pk.as_bytes().len(), 32);
}

#[test]
fn test_ed25519_keypair_from_bytes() {
    let kp1 = Ed25519KeyPair::generate();
    let secret = kp1.secret_bytes();

    let kp2 = Ed25519KeyPair::from_bytes(&secret).unwrap();

    assert_eq!(kp1.public_key().as_bytes(), kp2.public_key().as_bytes());
}

#[test]
fn test_ed25519_sign_verify() {
    let kp = Ed25519KeyPair::generate();
    let message = b"test message";

    let signature = kp.sign(message);
    kp.public_key().verify(message, &signature).unwrap();
}

#[test]
fn test_ed25519_verify_wrong_message() {
    let kp = Ed25519KeyPair::generate();

    let signature = kp.sign(b"original message");
    let result = kp.public_key().verify(b"different message", &signature);

    assert!(matches!(result, Err(CryptoError::InvalidSignature)));
}

#[test]
fn test_ed25519_signature_roundtrip() {
    let kp = Ed25519KeyPair::generate();
    let signature = kp.sign(b"message");

    let bytes = signature.to_bytes();
    let recovered = Ed25519Signature::from_bytes(&bytes).unwrap();

    kp.public_key().verify(b"message", &recovered).unwrap();
}
