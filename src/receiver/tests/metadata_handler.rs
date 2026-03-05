#[cfg(test)]
mod tests {
    use crate::receiver::metadata_handler::{MetadataError, dmap_tags, parse_dmap_metadata};

    #[test]
    fn test_parse_valid_metadata() {
        let mut data = Vec::new();
        // Title: "Song Title"
        data.extend_from_slice(dmap_tags::ITEM_NAME);
        data.extend_from_slice(&10u32.to_be_bytes());
        data.extend_from_slice(b"Song Title");

        let metadata = parse_dmap_metadata(&data).unwrap();
        assert_eq!(metadata.title.unwrap(), "Song Title");
    }

    #[test]
    fn test_parse_incomplete_data() {
        let mut data = Vec::new();
        data.extend_from_slice(dmap_tags::ITEM_NAME);
        data.extend_from_slice(&100u32.to_be_bytes()); // Length 100
        data.extend_from_slice(b"Short"); // But only 5 bytes provided

        let result = parse_dmap_metadata(&data);
        assert!(matches!(result, Err(MetadataError::IncompleteData)));
    }

    #[test]
    fn test_parse_overflow_length() {
        // This test simulates a potential overflow scenario.
        // On a 64-bit system, usize::MAX is huge, so we can't easily allocate a buffer that big to
        // trigger "real" OOB if we just used `offset + length`.
        // However, if we were on a 32-bit system, `length = u32::MAX` could wrap.
        // The fix uses `checked_add`, so we want to ensure that if `offset + length` overflows
        // `usize`, it fails gracefully.

        // On 64-bit, u32::MAX + small_offset won't overflow usize (u64).
        // So this test mainly verifies that *if* it were to overflow (e.g. if we had a hypothetical
        // 32-bit test env), it handles it. But importantly, we can test that HUGE lengths
        // that *would* be valid `usize` but are clearly OOB of the slice are caught by the
        // `end_offset > data.len()` check. The `checked_add` protects against the
        // wrap-around case which would bypass the length check.

        let mut data = Vec::new();
        data.extend_from_slice(dmap_tags::ITEM_NAME);
        data.extend_from_slice(&u32::MAX.to_be_bytes()); // Max u32 length
        data.extend_from_slice(b"start");

        // With the fix:
        // offset (8) + length (u32::MAX) -> No overflow on 64-bit (8 + 4294967295 = 4294967303).
        // end_offset (4294967303) > data.len() (13) -> Returns IncompleteData.

        // Without the fix (on 32-bit):
        // offset (8) + length (u32::MAX) -> 7 (wrapped).
        // 7 > data.len() (13) is FALSE.
        // Slice &data[8..7] -> Panic!

        // Since we are likely running on 64-bit in this env, we can't easily reproduce the 32-bit
        // overflow panic without mocking `usize` or using a 32-bit target.
        // However, we CAN verify that the logic remains sound for normal "too big" values.

        let result = parse_dmap_metadata(&data);
        assert!(matches!(result, Err(MetadataError::IncompleteData)));
    }
}
