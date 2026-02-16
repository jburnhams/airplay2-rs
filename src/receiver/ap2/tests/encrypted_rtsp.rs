use crate::receiver::ap2::encrypted_channel::EncryptedChannel;
use crate::receiver::ap2::encrypted_rtsp::EncryptedRtspCodec;

#[test]
fn test_plaintext_mode() {
    let mut codec = EncryptedRtspCodec::new();

    let request = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
    codec.feed(request);

    let decoded = codec.decode().unwrap().unwrap();
    assert_eq!(decoded.headers.cseq(), Some(1));
}

#[test]
fn test_encrypted_mode() {
    // Create two codecs to simulate sender/receiver
    let key_a = [0x41u8; 32];
    let key_b = [0x42u8; 32];

    let mut sender_channel = EncryptedChannel::new(key_a, key_b);
    let mut receiver = EncryptedRtspCodec::new();
    receiver.enable_encryption(key_b, key_a);

    // Encrypt a request
    let request = b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
    let encrypted = sender_channel.encrypt(request).unwrap();

    // Decode on receiver
    receiver.feed(&encrypted);
    let decoded = receiver.decode().unwrap().unwrap();
    assert_eq!(decoded.headers.cseq(), Some(1));
}
