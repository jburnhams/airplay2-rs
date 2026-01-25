use crate::protocol::daap::dmap::{DmapEncoder, DmapTag, decode_dmap};

#[test]
fn test_encode_string() {
    let mut encoder = DmapEncoder::new();
    encoder.string(DmapTag::ItemName, "Test Song");

    let data = encoder.finish();

    // Tag (4) + Length (4) + "Test Song" (9) = 17 bytes
    assert_eq!(data.len(), 17);
    assert_eq!(&data[0..4], b"minm");
    assert_eq!(u32::from_be_bytes([data[4], data[5], data[6], data[7]]), 9);
    assert_eq!(&data[8..], b"Test Song");
}

#[test]
fn test_encode_decode_roundtrip() {
    let mut encoder = DmapEncoder::new();
    encoder.string(DmapTag::ItemName, "My Track");
    encoder.string(DmapTag::SongArtist, "Artist Name");

    let data = encoder.finish();
    let decoded = decode_dmap(&data).unwrap();

    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].0, "minm");
    assert_eq!(decoded[0].1, "My Track");
    assert_eq!(decoded[1].0, "asar");
    assert_eq!(decoded[1].1, "Artist Name");
}
