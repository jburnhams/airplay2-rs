use super::{PairingError, TlvDecoder, TlvEncoder, TlvType, TransientPairing, tlv::*};

#[test]
fn test_tlv_encode_simple() {
    let encoded = TlvEncoder::new().add_state(1).add_method(0).build();

    assert_eq!(
        encoded,
        vec![
            0x06, 0x01, 0x01, // State = 1
            0x00, 0x01, 0x00, // Method = 0
        ]
    );
}

#[test]
fn test_tlv_decode_simple() {
    let data = vec![0x06, 0x01, 0x01, 0x00, 0x01, 0x00];
    let decoder = TlvDecoder::decode(&data).unwrap();

    assert_eq!(decoder.get_state().unwrap(), 1);
    assert_eq!(decoder.get(TlvType::Method), Some(&[0u8][..]));
}

#[test]
fn test_tlv_fragmentation() {
    // Data longer than 255 bytes should be fragmented
    let long_data = vec![0xAA; 300];
    let encoded = TlvEncoder::new()
        .add(TlvType::PublicKey, &long_data)
        .build();

    // Should have two TLV entries
    // Type (1) + Len (1) + Chunk (255) + Type (1) + Len (1) + Chunk (45)
    // 0x03, 0xFF, [255 bytes], 0x03, 0x2D, [45 bytes]

    assert_eq!(encoded[0], TlvType::PublicKey as u8);
    assert_eq!(encoded[1], 255); // First chunk is max size

    // Check second chunk start
    // 2 (header) + 255 = 257 index
    assert_eq!(encoded[257], TlvType::PublicKey as u8);
    assert_eq!(encoded[258], 45); // 300 - 255 = 45

    // Decode should reassemble
    let decoder = TlvDecoder::decode(&encoded).unwrap();
    let decoded = decoder.get(TlvType::PublicKey).unwrap();
    assert_eq!(decoded, &long_data[..]);
}

#[test]
fn test_tlv_error_detection() {
    let data = vec![0x07, 0x01, 0x02]; // Error = 2
    let decoder = TlvDecoder::decode(&data).unwrap();

    assert!(decoder.has_error());
    assert_eq!(decoder.get_error(), Some(2));
}

#[test]
fn test_tlv_missing_field() {
    let data = vec![0x06, 0x01, 0x01]; // Only state
    let decoder = TlvDecoder::decode(&data).unwrap();

    let result = decoder.get_required(TlvType::PublicKey);
    assert!(matches!(result, Err(TlvError::MissingField(_))));
}

#[test]
fn test_transient_start() {
    let mut pairing = TransientPairing::new().unwrap();
    let m1 = pairing.start().unwrap();

    let decoder = TlvDecoder::decode(&m1).unwrap();
    assert_eq!(decoder.get_state().unwrap(), 1);
    assert!(decoder.get(TlvType::PublicKey).is_some());
}

#[test]
fn test_transient_invalid_state() {
    let mut pairing = TransientPairing::new().unwrap();

    // Try to process M2 without starting
    let result = pairing.process_m2(&[]);
    assert!(matches!(result, Err(PairingError::InvalidState { .. })));
}

#[test]
fn test_transient_device_error() {
    let mut pairing = TransientPairing::new().unwrap();
    pairing.start().unwrap();

    // Simulate device error response
    let m2 = TlvEncoder::new()
        .add_state(2)
        .add_byte(TlvType::Error, errors::AUTHENTICATION)
        .build();

    let result = pairing.process_m2(&m2);
    assert!(matches!(result, Err(PairingError::DeviceError { code: 2 })));
}
