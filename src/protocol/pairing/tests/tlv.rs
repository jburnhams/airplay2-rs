use crate::protocol::pairing::tlv::{TlvDecoder, TlvEncoder, TlvError, TlvType};

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
    assert_eq!(encoded[0], TlvType::PublicKey as u8);
    assert_eq!(encoded[1], 255); // First chunk is max size
    // 255 bytes of data
    // Then next chunk
    assert_eq!(encoded[255 + 2], TlvType::PublicKey as u8);
    assert_eq!(encoded[255 + 3], 45); // 300 - 255 = 45

    // Decode should reassemble
    let tlv_decoder = TlvDecoder::decode(&encoded).unwrap();
    let decoded_bytes = tlv_decoder.get(TlvType::PublicKey).unwrap();
    assert_eq!(decoded_bytes, &long_data[..]);
}

#[test]
fn test_tlv_fragmentation_multiple() {
    // 3 fragments: 255 + 255 + 10
    let long_data = vec![0xAA; 520];

    let encoded = TlvEncoder::new()
        .add(TlvType::PublicKey, &long_data)
        .build();

    // Check structure
    // Frag 1: Type + Len(255) + 255 bytes
    // Frag 2: Type + Len(255) + 255 bytes
    // Frag 3: Type + Len(10) + 10 bytes
    // Total len: (1+1+255) * 2 + (1+1+10) = 514 + 12 = 526 bytes.
    assert_eq!(encoded.len(), 526);

    let decoder = TlvDecoder::decode(&encoded).unwrap();
    let decoded_data = decoder.get(TlvType::PublicKey).unwrap();
    assert_eq!(decoded_data, &long_data[..]);
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
