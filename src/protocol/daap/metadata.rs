//! Track metadata for RAOP

use super::dmap::{DmapEncoder, DmapTag, DmapValue};

/// Track metadata information
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
    /// Track number
    pub track_number: Option<u32>,
    /// Disc number
    pub disc_number: Option<u32>,
    /// Year
    pub year: Option<u32>,
    /// Duration in milliseconds
    pub duration_ms: Option<u32>,
}

impl TrackMetadata {
    /// Create new empty metadata
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create builder
    #[must_use]
    pub fn builder() -> MetadataBuilder {
        MetadataBuilder::new()
    }

    /// Encode as DMAP for `SET_PARAMETER`
    #[must_use]
    pub fn encode_dmap(&self) -> Vec<u8> {
        let mut encoder = DmapEncoder::new();

        // Wrap in mlit (listing item) container
        let mut item_encoder = DmapEncoder::new();

        if let Some(ref title) = self.title {
            item_encoder.string(DmapTag::ItemName, title);
        }

        if let Some(ref artist) = self.artist {
            item_encoder.string(DmapTag::SongArtist, artist);
        }

        if let Some(ref album) = self.album {
            item_encoder.string(DmapTag::SongAlbum, album);
        }

        if let Some(ref genre) = self.genre {
            item_encoder.string(DmapTag::SongGenre, genre);
        }

        if let Some(track) = self.track_number {
            item_encoder.int(DmapTag::SongTrackNumber, i64::from(track));
        }

        if let Some(disc) = self.disc_number {
            item_encoder.int(DmapTag::SongDiscNumber, i64::from(disc));
        }

        if let Some(year) = self.year {
            item_encoder.int(DmapTag::SongYear, i64::from(year));
        }

        if let Some(duration) = self.duration_ms {
            item_encoder.int(DmapTag::SongTime, i64::from(duration));
        }

        let item_data = item_encoder.finish();

        // Wrap the item data in mlit tag
        encoder.encode_tag(DmapTag::ListingItem, &DmapValue::Raw(item_data));

        encoder.finish()
    }

    /// Check if metadata is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.artist.is_none()
            && self.album.is_none()
            && self.genre.is_none()
            && self.track_number.is_none()
            && self.duration_ms.is_none()
    }
}

/// Builder for track metadata
pub struct MetadataBuilder {
    metadata: TrackMetadata,
}

impl MetadataBuilder {
    /// Create new builder
    #[must_use]
    pub fn new() -> Self {
        Self {
            metadata: TrackMetadata::new(),
        }
    }

    /// Set title
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.metadata.title = Some(title.into());
        self
    }

    /// Set artist
    #[must_use]
    pub fn artist(mut self, artist: impl Into<String>) -> Self {
        self.metadata.artist = Some(artist.into());
        self
    }

    /// Set album
    #[must_use]
    pub fn album(mut self, album: impl Into<String>) -> Self {
        self.metadata.album = Some(album.into());
        self
    }

    /// Set genre
    #[must_use]
    pub fn genre(mut self, genre: impl Into<String>) -> Self {
        self.metadata.genre = Some(genre.into());
        self
    }

    /// Set track number
    #[must_use]
    pub fn track_number(mut self, track: u32) -> Self {
        self.metadata.track_number = Some(track);
        self
    }

    /// Set disc number
    #[must_use]
    pub fn disc_number(mut self, disc: u32) -> Self {
        self.metadata.disc_number = Some(disc);
        self
    }

    /// Set year
    #[must_use]
    pub fn year(mut self, year: u32) -> Self {
        self.metadata.year = Some(year);
        self
    }

    /// Set duration in milliseconds
    #[must_use]
    pub fn duration_ms(mut self, duration: u32) -> Self {
        self.metadata.duration_ms = Some(duration);
        self
    }

    /// Build the metadata
    #[must_use]
    pub fn build(self) -> TrackMetadata {
        self.metadata
    }
}

impl Default for MetadataBuilder {
    fn default() -> Self {
        Self::new()
    }
}
