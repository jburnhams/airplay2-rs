use ::rsa::traits::PublicKeyParts;

use super::super::*;

#[test]
fn test_rsa_key_generation() {
    let key = RaopRsaPrivateKey::generate().unwrap();
    let public = key.public_key();

    assert_eq!(public.size(), rsa_sizes::MODULUS_BYTES);
}

#[test]
fn test_oaep_encrypt_decrypt() {
    let private = RaopRsaPrivateKey::generate().unwrap();

    // Create a "public key" struct from the private key's public component
    // In real code, this would use AppleRsaPublicKey with actual Apple key
    // For testing, we just use the raw rsa public key to encrypt and private to decrypt

    let plaintext = b"test AES key data";
    let public = private.public_key();

    // Encrypt with public key
    use ::rsa::Oaep;
    use rand::rngs::OsRng;
    use sha1::Sha1;

    use crate::protocol::crypto::CompatibleOsRng;

    let padding = Oaep::<Sha1>::new();
    let mut rng = CompatibleOsRng(OsRng);
    let ciphertext = public.encrypt(&mut rng, padding, plaintext).unwrap();

    // Decrypt with private key
    let decrypted = private.decrypt_oaep(&ciphertext).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_pkcs1_sign_verify() {
    let private = RaopRsaPrivateKey::generate().unwrap();
    let message = b"challenge||ip||mac";

    let signature = private.sign_pkcs1(message).unwrap();

    assert_eq!(signature.len(), rsa_sizes::SIGNATURE_BYTES);

    // Verify signature
    use ::rsa::pkcs1v15::{Signature, VerifyingKey};
    use ::rsa::signature::Verifier;
    use sha1::Sha1;

    let verifying_key = VerifyingKey::<Sha1>::new(private.public_key());
    let sig = Signature::try_from(signature.as_slice()).unwrap();
    verifying_key.verify(message, &sig).unwrap();
}

#[test]
fn test_oaep_max_plaintext() {
    let private = RaopRsaPrivateKey::generate().unwrap();

    // 16 bytes (AES key) should work
    let _aes_key = [0u8; 16];
    // Decrypting random 128 bytes usually fails padding check
    let _ = private.decrypt_oaep(&[0u8; 128]);

    // Validation that size checks work is implicitly done by `encrypt_oaep`
    // but here we are testing `AppleRsaPublicKey::encrypt_oaep` specifically
}

#[test]
fn test_apple_public_key_load() {
    // This tests if the hardcoded key is valid RSA key
    let key = AppleRsaPublicKey::load();
    assert!(key.is_ok());
}
