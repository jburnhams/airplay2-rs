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
            // Extract known fields
            if let Some(title) = Self::get_string(&parsed, DmapTag::ItemName) {
                metadata.title = Some(title);
            }
            if let Some(artist) = Self::get_string(&parsed, DmapTag::SongArtist) {
                metadata.artist = Some(artist);
            }
            if let Some(album) = Self::get_string(&parsed, DmapTag::SongAlbum) {
                metadata.album = Some(album);
            }
            if let Some(genre) = Self::get_string(&parsed, DmapTag::SongGenre) {
                metadata.genre = Some(genre);
            }
            if let Some(duration) = Self::get_u32(&parsed, DmapTag::SongTime) {
                metadata.duration_ms = Some(duration);
            }
            if let Some(track) = Self::get_u32(&parsed, DmapTag::SongTrackNumber) {
                metadata.track_number = Some(track);
            }
            if let Some(disc) = Self::get_u32(&parsed, DmapTag::SongDiscNumber) {
                metadata.disc_number = Some(disc);
            }

            tracing::debug!("Metadata updated: {:?}", *metadata);
        }

        Ok(())
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

    fn get_string(dmap: &DmapValue, tag: DmapTag) -> Option<String> {
        // Navigate DMAP structure to find key
        if let DmapValue::Container(items) = dmap {
            for (k, v) in items {
                if *k == tag {
                    if let DmapValue::String(s) = v {
                        return Some(s.clone());
                    }
                } else if let DmapValue::Container(_) = v {
                    // Recursive search
                    if let Some(s) = Self::get_string(v, tag) {
                        return Some(s);
                    }
                }
            }
        }
        None
    }

    fn get_u32(dmap: &DmapValue, tag: DmapTag) -> Option<u32> {
        if let DmapValue::Container(items) = dmap {
            for (k, v) in items {
                if *k == tag {
                    if let DmapValue::Int(i) = v {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        return Some(*i as u32);
                    }
                } else if let DmapValue::Container(_) = v {
                    if let Some(i) = Self::get_u32(v, tag) {
                        return Some(i);
                    }
                }
            }
        }
        None
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
