use crate::receiver::metadata_handler::parse_dmap_metadata;

#[test]
fn test_dmap_metadata_parsing() {
    // Construct minimal DMAP data
    // "minm" + length(5) + "Hello"
    let mut data = Vec::new();
    data.extend_from_slice(b"minm");
    data.extend_from_slice(&5u32.to_be_bytes());
    data.extend_from_slice(b"Hello");

    let metadata = parse_dmap_metadata(&data).unwrap();
    assert_eq!(metadata.title, Some("Hello".to_string()));
}
