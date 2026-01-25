use crate::protocol::daap::TrackMetadata;
use crate::protocol::daap::dmap::decode_dmap;

#[test]
fn test_metadata_builder() {
    let metadata = TrackMetadata::builder()
        .title("Song Title")
        .artist("The Artist")
        .album("Best Album")
        .track_number(1)
        .build();

    assert_eq!(metadata.title.as_deref(), Some("Song Title"));
    assert_eq!(metadata.artist.as_deref(), Some("The Artist"));
    assert_eq!(metadata.album.as_deref(), Some("Best Album"));
    assert_eq!(metadata.track_number, Some(1));
    assert_eq!(metadata.genre, None);
}

#[test]
fn test_metadata_encoding() {
    let metadata = TrackMetadata::builder()
        .title("Test")
        .track_number(5)
        .build();

    let encoded = metadata.encode_dmap();

    // Should be wrapped in mlit (listing item)
    // Structure: mlit (4) + length (4) + content
    assert_eq!(&encoded[0..4], b"mlit");
    let len = u32::from_be_bytes([encoded[4], encoded[5], encoded[6], encoded[7]]) as usize;
    assert_eq!(encoded.len(), 8 + len);

    // Decode inner content
    let inner_data = &encoded[8..];
    let decoded = decode_dmap(inner_data).unwrap();

    // Check tags
    // "minm" -> "Test"
    // "astn" -> 5

    // Note: decode_dmap decodes everything as string (via String::from_utf8_lossy)
    // Integer 5 is 1 byte: 0x05. utf8 lossy might not be "5".
    // But let's check the tags present.

    let has_title = decoded
        .iter()
        .any(|(tag, val)| tag == "minm" && val == "Test");
    assert!(has_title, "Missing title tag");

    let has_track = decoded.iter().any(|(tag, _)| tag == "astn");
    assert!(has_track, "Missing track number tag");
}
