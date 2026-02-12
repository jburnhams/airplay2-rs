//! Track metadata handling for `AirPlay` receiver
//!
//! Parses DMAP (Digital Media Access Protocol) encoded metadata
//! from `SET_PARAMETER` requests.

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
    /// Track number
    pub track_number: Option<u32>,
    /// Total tracks on album
    pub track_count: Option<u32>,
    /// Disc number
    pub disc_number: Option<u32>,
    /// Total discs
    pub disc_count: Option<u32>,
    /// Duration in milliseconds
    pub duration_ms: Option<u32>,
}

/// DMAP tag codes for metadata
pub(crate) mod dmap_tags {
    pub const ITEM_NAME: &[u8] = b"minm"; // Title
    pub const ITEM_ARTIST: &[u8] = b"asar"; // Artist
    pub const ITEM_ALBUM: &[u8] = b"asal"; // Album
    pub const ITEM_GENRE: &[u8] = b"asgn"; // Genre
    pub const TRACK_NUMBER: &[u8] = b"astn"; // Track number
    pub const TRACK_COUNT: &[u8] = b"astc"; // Track count
    pub const DISC_NUMBER: &[u8] = b"asdn"; // Disc number
    pub const DISC_COUNT: &[u8] = b"asdc"; // Disc count
    pub const DURATION: &[u8] = b"astm"; // Duration (ms)
}

/// Parse DMAP metadata from binary data
///
/// # Errors
/// Returns `MetadataError::InvalidFormat` if the DMAP data structure is corrupted or invalid.
/// Returns `MetadataError::IncompleteData` if the data buffer ends unexpectedly.
pub fn parse_dmap_metadata(data: &[u8]) -> Result<TrackMetadata, MetadataError> {
    let mut metadata = TrackMetadata::default();
    let mut offset = 0;

    while offset + 8 <= data.len() {
        // DMAP format: 4-byte tag, 4-byte length, data
        let tag = &data[offset..offset + 4];
        let length = u32::from_be_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;

        offset += 8;

        let end_offset = offset
            .checked_add(length)
            .ok_or(MetadataError::InvalidFormat)?;

        if end_offset > data.len() {
            return Err(MetadataError::IncompleteData);
        }

        let value = &data[offset..end_offset];
        offset = end_offset;

        // Parse based on tag
        match tag {
            t if t == dmap_tags::ITEM_NAME => {
                metadata.title = Some(String::from_utf8_lossy(value).into_owned());
            }
            t if t == dmap_tags::ITEM_ARTIST => {
                metadata.artist = Some(String::from_utf8_lossy(value).into_owned());
            }
            t if t == dmap_tags::ITEM_ALBUM => {
                metadata.album = Some(String::from_utf8_lossy(value).into_owned());
            }
            t if t == dmap_tags::ITEM_GENRE => {
                metadata.genre = Some(String::from_utf8_lossy(value).into_owned());
            }
            t if t == dmap_tags::TRACK_NUMBER && length >= 4 => {
                metadata.track_number =
                    Some(u32::from_be_bytes([value[0], value[1], value[2], value[3]]));
            }
            t if t == dmap_tags::TRACK_COUNT && length >= 4 => {
                metadata.track_count =
                    Some(u32::from_be_bytes([value[0], value[1], value[2], value[3]]));
            }
            t if t == dmap_tags::DISC_NUMBER && length >= 4 => {
                metadata.disc_number =
                    Some(u32::from_be_bytes([value[0], value[1], value[2], value[3]]));
            }
            t if t == dmap_tags::DISC_COUNT && length >= 4 => {
                metadata.disc_count =
                    Some(u32::from_be_bytes([value[0], value[1], value[2], value[3]]));
            }
            t if t == dmap_tags::DURATION && length >= 4 => {
                metadata.duration_ms =
                    Some(u32::from_be_bytes([value[0], value[1], value[2], value[3]]));
            }
            _ => {
                // Unknown tag, skip
            }
        }
    }

    Ok(metadata)
}

/// Errors parsing DMAP metadata
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    /// Invalid DMAP structure or format
    #[error("Invalid DMAP format")]
    InvalidFormat,

    /// Data buffer ended unexpectedly
    #[error("Incomplete data")]
    IncompleteData,
}
