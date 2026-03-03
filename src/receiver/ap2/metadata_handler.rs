//! Metadata and Artwork Handling

use std::sync::{Arc, PoisonError, RwLock};

use crate::protocol::daap::dmap::{DmapParser, DmapTag, DmapValue};

/// Track metadata
#[derive(Debug, Clone, Default)]
pub struct TrackMetadata {
    /// Track title
    pub title: Option<String>,
    /// Artist name
    pub artist: Option<String>,
    /// Album name
    pub album: Option<String>,
    /// Genre
    pub genre: Option<String>,
    /// Track duration in milliseconds
    pub duration_ms: Option<u32>,
    /// Track number
    pub track_number: Option<u32>,
    /// Disc number
    pub disc_number: Option<u32>,
}

/// Artwork data
#[derive(Debug, Clone)]
pub struct Artwork {
    /// Image data
    pub data: Vec<u8>,
    /// MIME type (e.g. "image/jpeg")
    pub mime_type: String,
}

/// Metadata controller
pub struct MetadataController {
    metadata: Arc<RwLock<TrackMetadata>>,
    artwork: Arc<RwLock<Option<Artwork>>>,
}

impl MetadataController {
    /// Create a new metadata controller
    #[must_use]
    pub fn new() -> Self {
        Self {
            metadata: Arc::new(RwLock::new(TrackMetadata::default())),
            artwork: Arc::new(RwLock::new(None)),
        }
    }

    /// Parse and update metadata from DMAP data
    ///
    /// # Errors
    ///
    /// Returns `MetadataError` if parsing fails.
    pub fn update_metadata(&self, dmap_data: &[u8]) -> Result<(), MetadataError> {
        let parsed =
            DmapParser::parse(dmap_data).map_err(|e| MetadataError::ParseError(e.to_string()))?;

        if let Ok(mut metadata) = self.metadata.write() {
            Self::extract_metadata(&parsed, &mut metadata);
            tracing::debug!("Metadata updated: {:?}", *metadata);
        }

        Ok(())
    }

    /// Recursively extract metadata fields from DMAP value
    fn extract_metadata(value: &DmapValue, metadata: &mut TrackMetadata) {
        if let DmapValue::Container(items) = value {
            for (tag, val) in items {
                match tag {
                    DmapTag::ItemName => {
                        if let DmapValue::String(s) = val {
                            metadata.title = Some(s.clone());
                        }
                    }
                    DmapTag::SongArtist => {
                        if let DmapValue::String(s) = val {
                            metadata.artist = Some(s.clone());
                        }
                    }
                    DmapTag::SongAlbum => {
                        if let DmapValue::String(s) = val {
                            metadata.album = Some(s.clone());
                        }
                    }
                    DmapTag::SongGenre => {
                        if let DmapValue::String(s) = val {
                            metadata.genre = Some(s.clone());
                        }
                    }
                    DmapTag::SongTime => {
                        if let DmapValue::Int(i) = val {
                            #[allow(
                                clippy::cast_possible_truncation,
                                clippy::cast_sign_loss,
                                reason = "DMAP ints fit in u32"
                            )]
                            {
                                metadata.duration_ms = Some(*i as u32);
                            }
                        }
                    }
                    DmapTag::SongTrackNumber => {
                        if let DmapValue::Int(i) = val {
                            #[allow(
                                clippy::cast_possible_truncation,
                                clippy::cast_sign_loss,
                                reason = "DMAP ints fit in u32"
                            )]
                            {
                                metadata.track_number = Some(*i as u32);
                            }
                        }
                    }
                    DmapTag::SongDiscNumber => {
                        if let DmapValue::Int(i) = val {
                            #[allow(
                                clippy::cast_possible_truncation,
                                clippy::cast_sign_loss,
                                reason = "DMAP ints fit in u32"
                            )]
                            {
                                metadata.disc_number = Some(*i as u32);
                            }
                        }
                    }
                    _ => {
                        // Recursively check containers
                        if let DmapValue::Container(_) = val {
                            Self::extract_metadata(val, metadata);
                        }
                    }
                }
            }
        }
    }

    /// Update artwork
    pub fn update_artwork(&self, data: Vec<u8>, mime_type: String) {
        if let Ok(mut artwork) = self.artwork.write() {
            *artwork = Some(Artwork { data, mime_type });
        }
    }

    /// Get current metadata
    #[must_use]
    pub fn metadata(&self) -> TrackMetadata {
        self.metadata
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Get current artwork
    #[must_use]
    pub fn artwork(&self) -> Option<Artwork> {
        self.artwork
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Clear metadata and artwork
    pub fn clear(&self) {
        if let Ok(mut m) = self.metadata.write() {
            *m = TrackMetadata::default();
        }
        if let Ok(mut a) = self.artwork.write() {
            *a = None;
        }
    }
}

impl Default for MetadataController {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors for metadata handling
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    /// Failed to parse DMAP data
    #[error("Failed to parse DMAP: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::daap::{DmapEncoder, DmapTag};

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
}
