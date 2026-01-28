use crate::protocol::daap::TrackMetadata;

#[derive(Debug, PartialEq)]
enum DmapTestValue {
    String(String),
    Int(i64),
    Container(Vec<(String, DmapTestValue)>),
    Raw(Vec<u8>),
}

/// Helper to decode DMAP data for verification
fn decode_dmap_full(data: &[u8]) -> Result<Vec<(String, DmapTestValue)>, String> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        if pos + 8 > data.len() {
            return Err("Unexpected end of data header".to_string());
        }

        let tag = std::str::from_utf8(&data[pos..pos + 4])
            .map_err(|_| "Invalid tag encoding".to_string())?
            .to_string();

        let len = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;

        pos += 8;

        if pos + len > data.len() {
            return Err("Unexpected end of data body".to_string());
        }

        let value_bytes = &data[pos..pos + len];

        // Heuristic decoding based on tag
        let value = match tag.as_str() {
            // Containers
            "mlcl" | "mlit" | "adbs" => {
                let inner = decode_dmap_full(value_bytes)?;
                DmapTestValue::Container(inner)
            }
            // Known integers (variable length)
            "astn" | "asdn" | "asyr" | "astm" => match len {
                1 => DmapTestValue::Int(i64::from(value_bytes[0])),
                2 => DmapTestValue::Int(i64::from(i16::from_be_bytes([
                    value_bytes[0],
                    value_bytes[1],
                ]))),
                4 => DmapTestValue::Int(i64::from(i32::from_be_bytes([
                    value_bytes[0],
                    value_bytes[1],
                    value_bytes[2],
                    value_bytes[3],
                ]))),
                8 => DmapTestValue::Int(i64::from_be_bytes([
                    value_bytes[0],
                    value_bytes[1],
                    value_bytes[2],
                    value_bytes[3],
                    value_bytes[4],
                    value_bytes[5],
                    value_bytes[6],
                    value_bytes[7],
                ])),
                _ => return Err(format!("Invalid integer length for {tag}: {len}")),
            },
            // Known strings
            "minm" | "asar" | "asal" | "asgn" => {
                let s = String::from_utf8(value_bytes.to_vec())
                    .map_err(|_| "Invalid UTF-8 string".to_string())?;
                DmapTestValue::String(s)
            }
            _ => {
                // Fallback
                if !value_bytes.is_empty()
                    && value_bytes
                        .iter()
                        .all(|&b| b.is_ascii_graphic() || b == b' ')
                {
                    if let Ok(s) = String::from_utf8(value_bytes.to_vec()) {
                        DmapTestValue::String(s)
                    } else {
                        DmapTestValue::Raw(value_bytes.to_vec())
                    }
                } else {
                    DmapTestValue::Raw(value_bytes.to_vec())
                }
            }
        };

        result.push((tag, value));
        pos += len;
    }

    Ok(result)
}

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
