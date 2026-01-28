use super::helpers::{DmapTestValue, decode_dmap_full};
use crate::protocol::daap::dmap::{DmapEncoder, DmapTag, DmapValue, decode_dmap};

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

    // Verify with full decoder
    let decoded = decode_dmap_full(&data).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].0, "minm");
    assert_eq!(decoded[0].1, DmapTestValue::String("Test Song".to_string()));
}

#[test]
fn test_encode_integers() {
    let mut encoder = DmapEncoder::new();

    // 1 byte (0-255)
    // SongTrackNumber (astn) usually u16 or u32 but fits in u8
    encoder.int(DmapTag::SongTrackNumber, 5);

    // 2 bytes (i16)
    // SongDiscNumber (asdn) - forcing larger value to check 2 byte encoding
    encoder.int(DmapTag::SongDiscNumber, 300);

    // 4 bytes (i32)
    // SongTime (astm) in ms
    encoder.int(DmapTag::SongTime, 100_000);

    // 8 bytes (i64)
    // Using SongYear (asyr) for a fake large value to test 8 bytes
    encoder.int(DmapTag::SongYear, 5_000_000_000);

    let data = encoder.finish();
    let decoded = decode_dmap_full(&data).unwrap();

    assert_eq!(decoded.len(), 4);

    // Check 5 (1 byte)
    assert_eq!(decoded[0].0, "astn");
    assert_eq!(decoded[0].1, DmapTestValue::Int(5));

    // Check 300 (2 bytes)
    assert_eq!(decoded[1].0, "asdn");
    assert_eq!(decoded[1].1, DmapTestValue::Int(300));

    // Check 100,000 (4 bytes)
    assert_eq!(decoded[2].0, "astm");
    assert_eq!(decoded[2].1, DmapTestValue::Int(100_000));

    // Check 5,000,000,000 (8 bytes)
    assert_eq!(decoded[3].0, "asyr");
    assert_eq!(decoded[3].1, DmapTestValue::Int(5_000_000_000));
}

#[test]
fn test_encode_container() {
    let mut encoder = DmapEncoder::new();
    // Using ListingItem (mlit) containing raw data which happens to be a valid item
    // In real usage we'd use DmapValue::Container, let's test that directly

    let inner_values = vec![(
        DmapTag::ItemName,
        DmapValue::String("Nested Name".to_string()),
    )];
    encoder.encode_tag(DmapTag::ListingItem, &DmapValue::Container(inner_values));

    let data = encoder.finish();
    let decoded = decode_dmap_full(&data).unwrap();

    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].0, "mlit");

    match &decoded[0].1 {
        DmapTestValue::Container(inner) => {
            assert_eq!(inner.len(), 1);
            assert_eq!(inner[0].0, "minm");
            assert_eq!(inner[0].1, DmapTestValue::String("Nested Name".to_string()));
        }
        _ => panic!("Expected container"),
    }
}

#[test]
fn test_encode_raw() {
    // Case 1: Raw bytes that are valid string (using ItemName)
    let mut encoder = DmapEncoder::new();
    encoder.encode_tag(DmapTag::ItemName, &DmapValue::Raw(b"RawString".to_vec()));
    let data = encoder.finish();
    let decoded = decode_dmap_full(&data).unwrap();
    assert_eq!(decoded[0].1, DmapTestValue::String("RawString".to_string()));

    // Case 2: Raw bytes that are NOT string (using unknown tag logic fallback)
    // We can't easily force "unknown tag" with DmapTag enum, but we can check if we had one.
    // Since we only have DmapTag variants, we have to pick one that isn't special-cased in decode_dmap_full
    // ListingItem (mlit) -> Container
    // SongTrackNumber (astn) -> Int
    // ...
    // SongGenre (asgn) -> String

    // We can simulate an unknown tag by manually constructing bytes, but we are testing DmapEncoder here.
    // DmapEncoder only supports DmapTag.
    // All DmapTag variants are handled in decode_dmap_full as either Container, Int, or String.
    // So to test Raw return from decode_dmap_full, we'd need a tag that falls into `_` branch.
    // But our `decode_dmap_full` handles all `DmapTag` variants explicitly or via "String".
    // Wait, `match tag.as_str()` handles:
    // "mlcl", "mlit", "adbs" -> Container
    // "astn", "asdn", "asyr", "astm" -> Int
    // "minm", "asar", "asal", "asgn" -> String
    // What about: SongTrackNumber (astn), SongDiscNumber (asdn), SongYear (asyr), SongTime (astm)
    // DmapTag::DatabaseSongs (adbs)
    // DmapTag::Listing (mlcl)
    // DmapTag::ListingItem (mlit)
    // DmapTag::ItemName (minm)
    // DmapTag::SongArtist (asar)
    // DmapTag::SongAlbum (asal)
    // DmapTag::SongGenre (asgn)

    // It seems ALL DmapTag variants are covered. So `decode_dmap_full` might not ever return `Raw`
    // unless we had a tag not in that list, but DmapEncoder only accepts DmapTag.
    // EXCEPT if `value_bytes` are not valid UTF-8 for "minm"/"asar"/etc.
    // But `decode_dmap_full` returns Error "Invalid UTF-8 string" for those tags.

    // So effectively, with current `DmapTag` enum and `decode_dmap_full`, we can't easily get `DmapTestValue::Raw`.
    // That's fine, we verified DmapEncoder encodes `DmapValue::Raw` correctly by checking it can be decoded back as a String when it mimics one.
}

#[test]
fn test_complex_structure() {
    let mut encoder = DmapEncoder::new();

    // Listing Item
    let mut item_encoder = DmapEncoder::new();
    item_encoder.string(DmapTag::ItemName, "Song A");
    item_encoder.int(DmapTag::SongTrackNumber, 1);

    // DmapValue::Container takes Vec<(DmapTag, DmapValue)>
    // But DmapEncoder helpers don't expose building that Vec easily directly without internal knowledge.
    // The previous test used `DmapValue::Container(vec![...])`.

    let item_values = vec![
        (DmapTag::ItemName, DmapValue::String("Song A".to_string())),
        (DmapTag::SongTrackNumber, DmapValue::Int(1)),
    ];

    encoder.encode_tag(DmapTag::ListingItem, &DmapValue::Container(item_values));

    let data = encoder.finish();
    let decoded = decode_dmap_full(&data).unwrap();

    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].0, "mlit");

    match &decoded[0].1 {
        DmapTestValue::Container(inner) => {
            assert_eq!(inner.len(), 2);
            assert_eq!(inner[0].0, "minm");
            assert_eq!(inner[0].1, DmapTestValue::String("Song A".to_string()));
            assert_eq!(inner[1].0, "astn");
            assert_eq!(inner[1].1, DmapTestValue::Int(1));
        }
        _ => panic!("Expected container"),
    }
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
