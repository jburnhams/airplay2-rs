//! DMAP (Digital Media Access Protocol) encoding

/// DMAP content codes (tags)
#[derive(Debug, Clone, Copy)]
pub enum DmapTag {
    /// Item name (track title)
    ItemName,
    /// Song artist
    SongArtist,
    /// Song album
    SongAlbum,
    /// Song genre
    SongGenre,
    /// Song track number
    SongTrackNumber,
    /// Song disc number
    SongDiscNumber,
    /// Song year
    SongYear,
    /// Song time (duration in ms)
    SongTime,
    /// Container listing
    Listing,
    /// Listing item
    ListingItem,
    /// Database songs
    DatabaseSongs,
}

impl DmapTag {
    /// Get 4-character code for tag
    #[must_use]
    pub fn code(&self) -> &'static [u8; 4] {
        match self {
            Self::ItemName => b"minm",
            Self::SongArtist => b"asar",
            Self::SongAlbum => b"asal",
            Self::SongGenre => b"asgn",
            Self::SongTrackNumber => b"astn",
            Self::SongDiscNumber => b"asdn",
            Self::SongYear => b"asyr",
            Self::SongTime => b"astm",
            Self::Listing => b"mlcl",
            Self::ListingItem => b"mlit",
            Self::DatabaseSongs => b"adbs",
        }
    }
}

/// DMAP value types
pub enum DmapValue {
    /// String value (UTF-8)
    String(String),
    /// Integer value (various sizes)
    Int(i64),
    /// Container (nested DMAP)
    Container(Vec<(DmapTag, DmapValue)>),
    /// Raw bytes
    Raw(Vec<u8>),
}

/// DMAP encoder
pub struct DmapEncoder {
    buffer: Vec<u8>,
}

impl DmapEncoder {
    /// Create new encoder
    #[must_use]
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Encode a tag-value pair
    pub fn encode_tag(&mut self, tag: DmapTag, value: &DmapValue) {
        // Write 4-byte tag code
        self.buffer.extend_from_slice(tag.code());

        match value {
            DmapValue::String(s) => {
                // Write length (4 bytes, big-endian)
                #[allow(clippy::cast_possible_truncation)]
                let len = s.len() as u32;
                self.buffer.extend_from_slice(&len.to_be_bytes());
                // Write string bytes
                self.buffer.extend_from_slice(s.as_bytes());
            }
            DmapValue::Int(n) => {
                // Determine appropriate size
                if *n >= 0 && *n <= 255 {
                    self.buffer.extend_from_slice(&1u32.to_be_bytes());
                    #[allow(clippy::cast_possible_truncation)]
                    #[allow(clippy::cast_sign_loss)]
                    self.buffer.push(*n as u8);
                } else if i16::try_from(*n).is_ok() {
                    self.buffer.extend_from_slice(&2u32.to_be_bytes());
                    #[allow(clippy::cast_possible_truncation)]
                    self.buffer.extend_from_slice(&(*n as i16).to_be_bytes());
                } else if i32::try_from(*n).is_ok() {
                    self.buffer.extend_from_slice(&4u32.to_be_bytes());
                    #[allow(clippy::cast_possible_truncation)]
                    self.buffer.extend_from_slice(&(*n as i32).to_be_bytes());
                } else {
                    self.buffer.extend_from_slice(&8u32.to_be_bytes());
                    self.buffer.extend_from_slice(&n.to_be_bytes());
                }
            }
            DmapValue::Container(items) => {
                // Encode container contents first
                let mut inner = DmapEncoder::new();
                for (inner_tag, inner_value) in items {
                    inner.encode_tag(*inner_tag, inner_value);
                }
                let inner_data = inner.finish();

                // Write length and contents
                #[allow(clippy::cast_possible_truncation)]
                let len = inner_data.len() as u32;
                self.buffer.extend_from_slice(&len.to_be_bytes());
                self.buffer.extend_from_slice(&inner_data);
            }
            DmapValue::Raw(data) => {
                #[allow(clippy::cast_possible_truncation)]
                let len = data.len() as u32;
                self.buffer.extend_from_slice(&len.to_be_bytes());
                self.buffer.extend_from_slice(data);
            }
        }
    }

    /// Add string tag
    pub fn string(&mut self, tag: DmapTag, value: &str) {
        self.encode_tag(tag, &DmapValue::String(value.to_string()));
    }

    /// Add integer tag
    pub fn int(&mut self, tag: DmapTag, value: i64) {
        self.encode_tag(tag, &DmapValue::Int(value));
    }

    /// Finish encoding and return bytes
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.buffer
    }
}

impl Default for DmapEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode DMAP data (for testing/debugging)
///
/// # Errors
///
/// Returns `DmapDecodeError` if data is invalid.
#[allow(dead_code)]
pub fn decode_dmap(data: &[u8]) -> Result<Vec<(String, String)>, DmapDecodeError> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos + 8 <= data.len() {
        let tag =
            std::str::from_utf8(&data[pos..pos + 4]).map_err(|_| DmapDecodeError::InvalidTag)?;
        let len = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;

        pos += 8;

        if pos + len > data.len() {
            return Err(DmapDecodeError::UnexpectedEnd);
        }

        let value_bytes = &data[pos..pos + len];

        // Try to decode as string
        let value = String::from_utf8_lossy(value_bytes).to_string();

        result.push((tag.to_string(), value));
        pos += len;
    }

    Ok(result)
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum DmapDecodeError {
    #[error("invalid tag")]
    InvalidTag,
    #[error("unexpected end of data")]
    UnexpectedEnd,
}
