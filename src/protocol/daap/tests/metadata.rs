use super::helpers::{DmapTestValue, decode_dmap_full};
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
        .artist("Artist")
        .track_number(5)
        .duration_ms(180_000)
        .build();

    let encoded = metadata.encode_dmap();

    // Decode outer content
    let decoded = decode_dmap_full(&encoded).unwrap();

    // Expect top-level container (mlit)
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].0, "mlit");

    match &decoded[0].1 {
        DmapTestValue::Container(inner) => {
            // Check contents
            // "minm" -> "Test"
            // "asar" -> "Artist"
            // "astn" -> 5
            // "astm" -> 180000

            let mut found_title = false;
            let mut found_artist = false;
            let mut found_track = false;
            let mut found_duration = false;

            for (tag, val) in inner {
                match tag.as_str() {
                    "minm" => {
                        assert_eq!(val, &DmapTestValue::String("Test".to_string()));
                        found_title = true;
                    }
                    "asar" => {
                        assert_eq!(val, &DmapTestValue::String("Artist".to_string()));
                        found_artist = true;
                    }
                    "astn" => {
                        assert_eq!(val, &DmapTestValue::Int(5));
                        found_track = true;
                    }
                    "astm" => {
                        assert_eq!(val, &DmapTestValue::Int(180_000));
                        found_duration = true;
                    }
                    _ => {}
                }
            }

            assert!(found_title, "Missing title");
            assert!(found_artist, "Missing artist");
            assert!(found_track, "Missing track number");
            assert!(found_duration, "Missing duration");
        }
        _ => panic!("Expected container"),
    }
}

#[test]
fn test_metadata_encoding_legacy() {
    // Keep a test using the legacy decoder to ensure compatibility
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

    let has_title = decoded
        .iter()
        .any(|(tag, val)| tag == "minm" && val == "Test");
    assert!(has_title, "Missing title tag");

    let has_track = decoded.iter().any(|(tag, _)| tag == "astn");
    assert!(has_track, "Missing track number tag");
}

#[test]
fn test_full_metadata_encoding() {
    let metadata = TrackMetadata::builder()
        .title("Title")
        .artist("Artist")
        .album("Album")
        .genre("Genre")
        .track_number(1)
        .disc_number(2)
        .year(2023)
        .duration_ms(300_000)
        .build();

    let encoded = metadata.encode_dmap();
    let decoded = decode_dmap_full(&encoded).unwrap();

    if let DmapTestValue::Container(inner) = &decoded[0].1 {
        assert!(
            inner
                .iter()
                .any(|(t, v)| t == "minm" && v == &DmapTestValue::String("Title".to_string()))
        );
        assert!(
            inner
                .iter()
                .any(|(t, v)| t == "asar" && v == &DmapTestValue::String("Artist".to_string()))
        );
        assert!(
            inner
                .iter()
                .any(|(t, v)| t == "asal" && v == &DmapTestValue::String("Album".to_string()))
        );
        assert!(
            inner
                .iter()
                .any(|(t, v)| t == "asgn" && v == &DmapTestValue::String("Genre".to_string()))
        );
        assert!(
            inner
                .iter()
                .any(|(t, v)| t == "astn" && v == &DmapTestValue::Int(1))
        );
        assert!(
            inner
                .iter()
                .any(|(t, v)| t == "asdn" && v == &DmapTestValue::Int(2))
        );
        assert!(
            inner
                .iter()
                .any(|(t, v)| t == "asyr" && v == &DmapTestValue::Int(2023))
        );
        assert!(
            inner
                .iter()
                .any(|(t, v)| t == "astm" && v == &DmapTestValue::Int(300_000))
        );
    } else {
        panic!("Expected container");
    }
}
