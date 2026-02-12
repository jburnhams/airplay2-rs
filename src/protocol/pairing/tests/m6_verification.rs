use crate::protocol::crypto::{
    ChaCha20Poly1305Cipher, Ed25519KeyPair, HkdfSha512, Nonce, SrpParams, SrpServer,
};
use crate::protocol::pairing::PairingStepResult;
use crate::protocol::pairing::setup::PairSetup;
use crate::protocol::pairing::tlv::{TlvDecoder, TlvEncoder, TlvType};

fn setup_to_m5() -> (PairSetup, Vec<u8>) {
    let mut setup = PairSetup::new();
    let pin = "3939";
    setup.set_pin(pin);
    setup.set_username("Pair-Setup");

    // M1
    let m1 = setup.start().expect("M1 failed");
    let tlv_m1 = TlvDecoder::decode(&m1).unwrap();
    assert_eq!(tlv_m1.get_state().unwrap(), 1);

    // Mock Server
    let salt = b"salt1234salt1234";
    let verifier = SrpServer::compute_verifier(
        b"Pair-Setup",
        pin.as_bytes(),
        salt,
        &SrpParams::RFC5054_3072,
    );
    let server = SrpServer::new(&verifier, &SrpParams::RFC5054_3072);

    // M2
    let m2 = TlvEncoder::new()
        .add_state(2)
        .add(TlvType::Salt, salt)
        .add(TlvType::PublicKey, server.public_key())
        .build();

    // Process M2 -> M3
    let res = setup.process_m2(&m2).expect("M2 processing failed");
    let PairingStepResult::SendData(m3) = res else {
        panic!("Expected SendData for M3")
    };

    let tlv_m3 = TlvDecoder::decode(&m3).unwrap();
    let client_pk = tlv_m3.get_required(TlvType::PublicKey).unwrap();
    let client_proof = tlv_m3.get_required(TlvType::Proof).unwrap();

    // Verify M3 -> M4
    let (session_key, server_proof) = server
        .verify_client(b"Pair-Setup", salt, client_pk, client_proof)
        .expect("Server verify failed");

    let m4 = TlvEncoder::new()
        .add_state(4)
        .add(TlvType::Proof, &server_proof)
        .build();

    // Process M4 -> M5
    let res = setup.process_m4(&m4).expect("M4 processing failed");
    match res {
        PairingStepResult::SendData(_) => (),
        _ => panic!("Expected SendData for M5"),
    }

    (setup, session_key.as_bytes().to_vec())
}

fn prepare_m6(session_key: &[u8], corrupt_signature: bool) -> Vec<u8> {
    let hkdf_enc = HkdfSha512::new(Some(b"Pair-Setup-Encrypt-Salt"), session_key);
    let encrypt_key = hkdf_enc
        .expand_fixed::<32>(b"Pair-Setup-Encrypt-Info")
        .unwrap();
    let cipher = ChaCha20Poly1305Cipher::new(&encrypt_key).unwrap();

    let server_ltpk = Ed25519KeyPair::generate();
    let identifier = b"AccessoryID";

    let hkdf_sign = HkdfSha512::new(Some(b"Pair-Setup-Accessory-Sign-Salt"), session_key);
    let accessory_key = hkdf_sign
        .expand_fixed::<32>(b"Pair-Setup-Accessory-Sign-Info")
        .unwrap();

    let mut sign_data = Vec::new();
    sign_data.extend_from_slice(&accessory_key);
    sign_data.extend_from_slice(identifier);
    sign_data.extend_from_slice(server_ltpk.public_key().as_bytes());

    let signature = server_ltpk.sign(&sign_data);
    let mut signature_bytes = signature.to_bytes();

    if corrupt_signature {
        signature_bytes[0] ^= 0xFF; // Corrupt it
    }

    let inner_tlv = TlvEncoder::new()
        .add(TlvType::Identifier, identifier)
        .add(TlvType::PublicKey, server_ltpk.public_key().as_bytes())
        .add(TlvType::Signature, &signature_bytes)
        .build();

    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..].copy_from_slice(b"PS-Msg06");
    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
    let encrypted = cipher.encrypt(&nonce, &inner_tlv).unwrap();

    TlvEncoder::new()
        .add_state(6)
        .add(TlvType::EncryptedData, &encrypted)
        .build()
}

#[test]
fn test_m6_verification_valid() {
    let (mut setup, session_key) = setup_to_m5();
    let m6 = prepare_m6(&session_key, false);

    let res = setup.process_m6(&m6).expect("M6 processing failed");
    match res {
        PairingStepResult::Complete(_) => (),
        _ => panic!("Expected Complete"),
    }
}

#[test]
fn test_m6_verification_invalid() {
    let (mut setup, session_key) = setup_to_m5();
    let m6 = prepare_m6(&session_key, true);

    let res = setup.process_m6(&m6);
    assert!(res.is_err(), "Should have failed verification");
}
