use super::super::*;

#[test]
fn test_x25519_key_exchange() {
    let alice = X25519KeyPair::generate();
    let bob = X25519KeyPair::generate();

    let alice_shared = alice.diffie_hellman(&bob.public_key());
    let bob_shared = bob.diffie_hellman(&alice.public_key());

    assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
}

#[test]
fn test_x25519_keypair_roundtrip() {
    let kp1 = X25519KeyPair::generate();
    let secret = kp1.secret_bytes();

    let kp2 = X25519KeyPair::from_bytes(&secret).unwrap();

    assert_eq!(kp1.public_key().as_bytes(), kp2.public_key().as_bytes());
}

#[test]
fn test_x25519_public_key_from_bytes() {
    let kp = X25519KeyPair::generate();
    let pk_bytes = kp.public_key().as_bytes().to_vec();

    let pk = X25519PublicKey::from_bytes(&pk_bytes).unwrap();

    assert_eq!(pk.as_bytes(), kp.public_key().as_bytes());
}
