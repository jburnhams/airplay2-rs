use crate::protocol::pairing::tlv::{TlvDecoder, TlvEncoder, TlvError, TlvType};

#[test]
fn test_tlv_zero_length() {
    let encoded = TlvEncoder::new().add(TlvType::Method, &[]).build();

    assert_eq!(encoded, vec![0x00, 0x00]);

    let decoder = TlvDecoder::decode(&encoded).unwrap();
    assert_eq!(decoder.get(TlvType::Method), Some(&[][..]));
}

#[test]
fn test_tlv_invalid_structure_incomplete_header() {
    let data = vec![0x00]; // Type but no length
    let result = TlvDecoder::decode(&data);
    assert!(matches!(result, Err(TlvError::BufferTooSmall)));
}

#[test]
fn test_tlv_invalid_structure_incomplete_value() {
    let data = vec![0x00, 0x05, 0x01, 0x02]; // Length 5, but only 2 bytes
    let result = TlvDecoder::decode(&data);
    assert!(matches!(result, Err(TlvError::BufferTooSmall)));
}

#[test]
fn test_tlv_mixed_valid_invalid() {
    let mut data = vec![
        0x06, 0x01, 0x01, // State = 1 (Valid)
    ];
    // Append incomplete TLV
    data.push(0x00);

    let result = TlvDecoder::decode(&data);
    assert!(matches!(result, Err(TlvError::BufferTooSmall)));
}

#[test]
fn test_tlv_large_fragmentation() {
    // Test multiple fragments
    let size = 600;
    let data = vec![0x42; size]; // 600 bytes of data

    let encoded = TlvEncoder::new().add(TlvType::Certificate, &data).build();

    // 255 + 255 + 90 = 600
    // Each chunk has 2 bytes overhead (type + len)
    // 3 chunks * 2 = 6 bytes overhead
    // Total size = 606
    assert_eq!(encoded.len(), 606);

    let decoder = TlvDecoder::decode(&encoded).unwrap();
    let decoded_data = decoder.get(TlvType::Certificate).unwrap();
    assert_eq!(decoded_data.len(), size);
    assert_eq!(decoded_data, &data[..]);
}

#[test]
fn test_tlv_multiple_types() {
    let encoded = TlvEncoder::new()
        .add_state(1)
        .add_method(0)
        .add(TlvType::Identifier, b"id")
        .build();

    let decoder = TlvDecoder::decode(&encoded).unwrap();
    assert_eq!(decoder.get_state().unwrap(), 1);
    assert_eq!(decoder.get(TlvType::Method), Some(&[0u8][..]));
    assert_eq!(decoder.get(TlvType::Identifier), Some(b"id".as_slice()));
}
