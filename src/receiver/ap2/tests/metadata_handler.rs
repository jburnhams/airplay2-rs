#[cfg(test)]
mod tests {
    use crate::protocol::daap::{DmapEncoder, DmapTag, DmapValue};
    use crate::receiver::ap2::metadata_handler::MetadataController;

    #[test]
    fn test_metadata_defaults() {
        let controller = MetadataController::new();
        let metadata = controller.metadata();

        assert!(metadata.title.is_none());
        assert!(metadata.artist.is_none());
    }

    #[test]
    fn test_artwork_update() {
        let controller = MetadataController::new();

        assert!(controller.artwork().is_none());

        controller.update_artwork(vec![1, 2, 3], "image/jpeg".into());

        let artwork = controller.artwork().unwrap();
        assert_eq!(artwork.data, vec![1, 2, 3]);
        assert_eq!(artwork.mime_type, "image/jpeg");
    }

    #[test]
    fn test_metadata_update() {
        let controller = MetadataController::new();
        let mut encoder = DmapEncoder::new();

        // Build DMAP packet. Note: DmapEncoder API was updated to take tag + value
        // The previous test code assumed a different API, so we fix it here.

        let mut inner = DmapEncoder::new();
        inner.string(DmapTag::ItemName, "Song Title");
        inner.string(DmapTag::SongArtist, "Artist Name");
        inner.int(DmapTag::SongTime, 3000);
        let inner_val = DmapValue::Container(vec![
            (DmapTag::ItemName, DmapValue::String("Song Title".into())),
            (DmapTag::SongArtist, DmapValue::String("Artist Name".into())),
            (DmapTag::SongTime, DmapValue::Int(3000)),
        ]);

        encoder.encode_tag(DmapTag::ListingItem, &inner_val);

        let data = encoder.finish();
        controller.update_metadata(&data).unwrap();

        let metadata = controller.metadata();
        assert_eq!(metadata.title.as_deref(), Some("Song Title"));
        assert_eq!(metadata.artist.as_deref(), Some("Artist Name"));
        assert_eq!(metadata.duration_ms, Some(3000));
    }

    #[test]
    fn test_metadata_clear() {
        let controller = MetadataController::new();

        // Update artwork and metadata
        controller.update_artwork(vec![1, 2, 3], "image/jpeg".into());
        let mut inner = DmapEncoder::new();
        inner.string(DmapTag::ItemName, "Song Title");
        let inner_val = DmapValue::Container(vec![(
            DmapTag::ItemName,
            DmapValue::String("Song Title".into()),
        )]);
        let mut encoder = DmapEncoder::new();
        encoder.encode_tag(DmapTag::ListingItem, &inner_val);
        controller.update_metadata(&encoder.finish()).unwrap();

        assert!(controller.artwork().is_some());
        assert!(controller.metadata().title.is_some());

        controller.clear();

        assert!(controller.artwork().is_none());
        assert!(controller.metadata().title.is_none());
    }
}
