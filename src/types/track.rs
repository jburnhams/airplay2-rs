/// Information about a track for playback
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackInfo {
    /// URL to audio content (HTTP/HTTPS)
    pub url: String,

    /// Track title
    pub title: String,

    /// Artist name
    pub artist: String,

    /// Album name
    pub album: Option<String>,

    /// URL to album artwork
    pub artwork_url: Option<String>,

    /// Track duration in seconds
    pub duration_secs: Option<f64>,

    /// Track number on album
    pub track_number: Option<u32>,

    /// Disc number
    pub disc_number: Option<u32>,

    /// Genre
    pub genre: Option<String>,

    /// Content identifier for queue management
    pub content_id: Option<String>,
}

impl TrackInfo {
    /// Create a new `TrackInfo` with required fields
    pub fn new(
        url: impl Into<String>,
        title: impl Into<String>,
        artist: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            title: title.into(),
            artist: artist.into(),
            ..Default::default()
        }
    }

    /// Builder method to set album
    #[must_use]
    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album = Some(album.into());
        self
    }

    /// Builder method to set artwork URL
    #[must_use]
    pub fn with_artwork(mut self, artwork_url: impl Into<String>) -> Self {
        self.artwork_url = Some(artwork_url.into());
        self
    }

    /// Builder method to set duration
    #[must_use]
    pub fn with_duration(mut self, duration_secs: f64) -> Self {
        self.duration_secs = Some(duration_secs);
        self
    }
}

/// A track in the playback queue with unique identifier
#[derive(Debug, Clone)]
pub struct QueueItem {
    /// Unique identifier for this queue position
    pub item_id: i32,

    /// Track information
    pub track: TrackInfo,
}
