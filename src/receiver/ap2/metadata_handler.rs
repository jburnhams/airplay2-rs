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
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            {
                                metadata.duration_ms = Some(*i as u32);
                            }
                        }
                    }
                    DmapTag::SongTrackNumber => {
                        if let DmapValue::Int(i) = val {
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            {
                                metadata.track_number = Some(*i as u32);
                            }
                        }
                    }
                    DmapTag::SongDiscNumber => {
                        if let DmapValue::Int(i) = val {
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
