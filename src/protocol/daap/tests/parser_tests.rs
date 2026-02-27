#[cfg(test)]
mod tests {
    use crate::protocol::daap::dmap::*;

    #[test]
    fn test_dmap_encode_decode_string() {
        let mut encoder = DmapEncoder::new();
        encoder.string(DmapTag::ItemName, "Test Title");

        let data = encoder.finish();
        let decoded = DmapParser::parse(&data).unwrap();

        match decoded {
            DmapValue::Container(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].0, DmapTag::ItemName);
                if let DmapValue::String(s) = &items[0].1 {
                    assert_eq!(s, "Test Title");
                } else {
                    panic!("Expected string value");
                }
            }
            _ => panic!("Expected container"),
        }
    }

    #[test]
    fn test_dmap_encode_decode_int() {
        let mut encoder = DmapEncoder::new();
        encoder.int(DmapTag::SongYear, 2023);

        let data = encoder.finish();
        let decoded = DmapParser::parse(&data).unwrap();

        match decoded {
            DmapValue::Container(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].0, DmapTag::SongYear);
                if let DmapValue::Int(i) = &items[0].1 {
                    assert_eq!(*i, 2023);
                } else {
                    panic!("Expected int value");
                }
            }
            _ => panic!("Expected container"),
        }
    }
}
