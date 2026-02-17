use crate::protocol::crypto::{ChaCha20Poly1305Cipher, Nonce};
use crate::protocol::rtp::{RtpHeader, RtpPacket};
use crate::receiver::ap2::rtp_decryptor::{Ap2RtpDecryptor, AudioDecoder, PcmDecoder};

#[test]
fn test_rtp_decryption() {
    let key = [0x42; 32];
    let decryptor = Ap2RtpDecryptor::new(key);

    // Create a dummy RTP packet
    let sequence = 0x1234;
    let timestamp = 0x56789ABC;
    let ssrc = 0xDEADBEEF;
    let payload_data = b"Hello AirPlay 2";

    let header = RtpHeader::new_audio(sequence, timestamp, ssrc, false);

    // Manually encrypt the payload to simulate sender
    // Nonce construction must match Ap2RtpDecryptor::build_nonce
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..8].copy_from_slice(&ssrc.to_be_bytes());
    nonce_bytes[8..10].copy_from_slice(&sequence.to_be_bytes());
    // remaining bytes are 0

    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

    // No AAD prefix by default
    // AAD is not used in the default case in `decrypt` unless prefix is set?
    // Wait, Ap2RtpDecryptor::decrypt says:
    // "Build AAD if configured" -> self.aad_prefix.as_ref().map(...)
    // If aad_prefix is None, it calls cipher.decrypt(&nonce, payload)
    // So NO AAD is used if prefix is not set.

    let encrypted_payload = cipher.encrypt(&nonce, payload_data).unwrap();

    let packet = RtpPacket::new(header, encrypted_payload);

    // Decrypt
    let decrypted = decryptor.decrypt(&packet).expect("Decryption failed");

    assert_eq!(decrypted, payload_data);
}

#[test]
fn test_rtp_decryption_with_aad() {
    let key = [0x99; 32];
    let mut decryptor = Ap2RtpDecryptor::new(key);
    let aad_prefix = vec![0xAA, 0xBB];
    decryptor.set_aad_prefix(aad_prefix.clone());

    let sequence = 0x1000;
    let timestamp = 0x2000;
    let ssrc = 0x3000;
    let payload_data = b"Encrypted with AAD";

    let header = RtpHeader::new_audio(sequence, timestamp, ssrc, false);

    // Nonce
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..8].copy_from_slice(&ssrc.to_be_bytes());
    nonce_bytes[8..10].copy_from_slice(&sequence.to_be_bytes());
    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();

    // AAD construction
    // Prefix + RTP header bytes
    let mut aad = aad_prefix;
    aad.extend_from_slice(&header.encode());

    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();
    let encrypted_payload = cipher
        .encrypt_with_aad(&nonce, &aad, payload_data)
        .unwrap();

    let packet = RtpPacket::new(header, encrypted_payload);

    let decrypted = decryptor.decrypt(&packet).expect("Decryption with AAD failed");

    assert_eq!(decrypted, payload_data);
}

#[test]
fn test_pcm_decoder_16bit() {
    let mut decoder = PcmDecoder::new(44100, 2, 16);

    // Two 16-bit samples (little endian)
    // 0x4000 = 16384
    // 0xC000 = -16384 (signed 16-bit)
    let data = [0x00, 0x40, 0x00, 0xC0];
    let samples = decoder.decode(&data).unwrap();

    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0], 16384);
    assert_eq!(samples[1], -16384);
}

#[test]
fn test_pcm_decoder_24bit() {
    let mut decoder = PcmDecoder::new(48000, 2, 24);

    // Two 24-bit samples (little endian)
    // We want output 16384 (0x4000).
    // Logic: value = i32::from_le_bytes([0, b0, b1, b2]) >> 16.
    // So we want value >> 16 = 0x4000.
    // value should be 0x40000000 (roughly).
    // i32 from bytes [0, b0, b1, b2] is 0x(b2)(b1)(b0)00.
    // So we want 0x(b2)(b1)(b0)00 >> 16 = 0x4000.
    // 0x(b2)(b1)(b0)00 = 0x40000000.
    // So b2=40, b1=00, b0=00.
    // Input: 00 00 40.

    let data = [0x00, 0x00, 0x40];
    let samples = decoder.decode(&data).unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0], 16384);
}
