use tokio::sync::mpsc;

use crate::protocol::crypto::{ChaCha20Poly1305Cipher, Nonce};
use crate::protocol::rtp::RtpHeader;
use crate::receiver::ap2::rtp_receiver::{RtpReceiver, RtpReceiverConfig};

#[tokio::test]
async fn test_rtp_receiver_process() {
    let (tx, mut rx) = mpsc::channel(10);
    let key = [0x42; 32];

    let config = RtpReceiverConfig {
        port: 7000,
        key,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        codec_type: 100, // PCM
    };

    let mut receiver = RtpReceiver::new(config, tx).unwrap();

    // Create valid encrypted packet
    let sequence = 0x10;
    let timestamp = 0x20;
    let ssrc = 0x30;
    // PCM 16-bit: 0x00, 0x40 = 16384; 0x00, 0xC0 = -16384
    let payload_data = [0x00, 0x40, 0x00, 0xC0];

    let header = RtpHeader::new_audio(sequence, timestamp, ssrc, false);

    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..8].copy_from_slice(&ssrc.to_be_bytes());
    nonce_bytes[8..10].copy_from_slice(&sequence.to_be_bytes());
    let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

    let encrypted_payload = cipher.encrypt(&nonce, &payload_data).unwrap();

    let mut packet_bytes = Vec::new();
    packet_bytes.extend_from_slice(&header.encode());
    packet_bytes.extend_from_slice(&encrypted_payload);

    // Process packet
    receiver.process_packet(&packet_bytes).unwrap();

    // Check stats
    assert_eq!(receiver.stats().packets_received, 1);
    assert_eq!(receiver.stats().packets_decrypted, 1);
    assert_eq!(receiver.stats().samples_decoded, 2);

    // Receive frame
    let frame = rx.recv().await.unwrap();
    assert_eq!(frame.sequence, sequence);
    assert_eq!(frame.timestamp, timestamp);
    assert_eq!(frame.samples.len(), 2);
    assert_eq!(frame.samples[0], 16384);
    assert_eq!(frame.samples[1], -16384);
}

#[tokio::test]
async fn test_rtp_receiver_sequence_gap() {
    let (tx, _rx) = mpsc::channel(10);
    let key = [0x42; 32];
    let config = RtpReceiverConfig {
        port: 7000,
        key,
        sample_rate: 44100,
        channels: 2,
        bits_per_sample: 16,
        codec_type: 100,
    };
    let mut receiver = RtpReceiver::new(config, tx).unwrap();

    let ssrc = 0x30;
    let cipher = ChaCha20Poly1305Cipher::new(&key).unwrap();

    // Packet 1 (Seq 1)
    {
        let sequence = 1;
        let header = RtpHeader::new_audio(sequence, 0, ssrc, false);
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..8].copy_from_slice(&ssrc.to_be_bytes());
        nonce_bytes[8..10].copy_from_slice(&sequence.to_be_bytes());
        let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
        let enc = cipher.encrypt(&nonce, &[]).unwrap();
        let mut buf = header.encode().to_vec();
        buf.extend_from_slice(&enc);
        receiver.process_packet(&buf).unwrap();
    }

    // Packet 3 (Seq 3) -> Gap detected
    {
        let sequence = 3;
        let header = RtpHeader::new_audio(sequence, 0, ssrc, false);
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..8].copy_from_slice(&ssrc.to_be_bytes());
        nonce_bytes[8..10].copy_from_slice(&sequence.to_be_bytes());
        let nonce = Nonce::from_bytes(&nonce_bytes).unwrap();
        let enc = cipher.encrypt(&nonce, &[]).unwrap();
        let mut buf = header.encode().to_vec();
        buf.extend_from_slice(&enc);
        receiver.process_packet(&buf).unwrap();
    }

    assert_eq!(receiver.stats().packets_received, 2);
    assert_eq!(receiver.stats().sequence_gaps, 1);
}
