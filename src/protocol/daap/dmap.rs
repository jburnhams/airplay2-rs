//! DMAP (Digital Media Access Protocol) encoding and decoding

use std::fmt;

/// DMAP content codes (tags)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Unknown tag
    Unknown([u8; 4]),
}

impl DmapTag {
    /// Get 4-character code for tag
    #[must_use]
    pub fn code(&self) -> [u8; 4] {
        match self {
            Self::ItemName => *b"minm",
            Self::SongArtist => *b"asar",
            Self::SongAlbum => *b"asal",
            Self::SongGenre => *b"asgn",
            Self::SongTrackNumber => *b"astn",
            Self::SongDiscNumber => *b"asdn",
            Self::SongYear => *b"asyr",
            Self::SongTime => *b"astm",
            Self::Listing => *b"mlcl",
            Self::ListingItem => *b"mlit",
            Self::DatabaseSongs => *b"adbs",
            Self::Unknown(code) => *code,
        }
    }

    /// Create tag from bytes
    #[must_use]
    pub fn from_bytes(bytes: [u8; 4]) -> Self {
        match &bytes {
            b"minm" => Self::ItemName,
            b"asar" => Self::SongArtist,
            b"asal" => Self::SongAlbum,
            b"asgn" => Self::SongGenre,
            b"astn" => Self::SongTrackNumber,
            b"asdn" => Self::SongDiscNumber,
            b"asyr" => Self::SongYear,
            b"astm" => Self::SongTime,
            b"mlcl" => Self::Listing,
            b"mlit" => Self::ListingItem,
            b"adbs" => Self::DatabaseSongs,
            _ => Self::Unknown(bytes),
        }
    }

    /// Check if this tag represents a container
    #[must_use]
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            Self::Listing | Self::ListingItem | Self::DatabaseSongs
        )
    }
}

impl fmt::Display for DmapTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        let s = std::str::from_utf8(&code).unwrap_or("????");
        write!(f, "{s}")
    }
}

/// DMAP value types
#[derive(Debug, Clone, PartialEq, Eq)]
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
        self.buffer.extend_from_slice(&tag.code());

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

/// DMAP parser
pub struct DmapParser;

impl DmapParser {
    /// Parse DMAP data into a structured value
    ///
    /// # Errors
    ///
    /// Returns `DmapDecodeError` if data is invalid.
    pub fn parse(data: &[u8]) -> Result<DmapValue, DmapDecodeError> {
        // Top level is implicitly a container of items
        let items = Self::parse_container(data)?;
        Ok(DmapValue::Container(items))
    }

    fn parse_container(mut data: &[u8]) -> Result<Vec<(DmapTag, DmapValue)>, DmapDecodeError> {
        let mut items = Vec::new();

        while !data.is_empty() {
            if data.len() < 8 {
                return Err(DmapDecodeError::UnexpectedEnd);
            }

            let tag_bytes: [u8; 4] = data[0..4].try_into().unwrap();
            let len = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
            data = &data[8..];

            if len > data.len() {
                return Err(DmapDecodeError::UnexpectedEnd);
            }

            let value_bytes = &data[0..len];
            data = &data[len..];

            let tag = DmapTag::from_bytes(tag_bytes);
            let value = if tag.is_container() {
                DmapValue::Container(Self::parse_container(value_bytes)?)
            } else {
                Self::parse_value(tag, value_bytes)?
            };

            items.push((tag, value));
        }

        Ok(items)
    }

    fn parse_value(tag: DmapTag, bytes: &[u8]) -> Result<DmapValue, DmapDecodeError> {
        // Heuristic based on tag type
        // Known integer types
        match tag {
            DmapTag::SongTrackNumber
            | DmapTag::SongDiscNumber
            | DmapTag::SongYear
            | DmapTag::SongTime => {
                let int_val = match bytes.len() {
                    1 => i64::from(bytes[0]),
                    2 => i64::from(i16::from_be_bytes(bytes.try_into().unwrap())),
                    4 => i64::from(i32::from_be_bytes(bytes.try_into().unwrap())),
                    8 => i64::from_be_bytes(bytes.try_into().unwrap()),
                    _ => return Err(DmapDecodeError::InvalidIntSize(bytes.len())),
                };
                return Ok(DmapValue::Int(int_val));
            }
            _ => {}
        }

        // Try parsing as UTF-8 string first
        if let Ok(s) = std::str::from_utf8(bytes) {
            // Check if it looks like a valid string (no control chars except rare ones)
            if s.chars().all(|c| !c.is_control()) {
                return Ok(DmapValue::String(s.to_string()));
            }
        }

        // Default to raw bytes
        Ok(DmapValue::Raw(bytes.to_vec()))
    }
}

/// Decode DMAP data (deprecated, use `DmapParser`)
///
/// # Errors
///
/// Returns `DmapDecodeError` if data is invalid.
#[allow(dead_code)]
pub fn decode_dmap(data: &[u8]) -> Result<Vec<(String, String)>, DmapDecodeError> {
    let value = DmapParser::parse(data)?;

    fn flatten(v: &DmapValue, acc: &mut Vec<(String, String)>) {
        if let DmapValue::Container(items) = v {
            for (tag, val) in items {
                match val {
                    DmapValue::String(s) => acc.push((tag.to_string(), s.clone())),
                    DmapValue::Int(i) => acc.push((tag.to_string(), i.to_string())),
                    DmapValue::Container(_) => flatten(val, acc),
                    DmapValue::Raw(bytes) => {
                        acc.push((tag.to_string(), format!("<{} bytes>", bytes.len())));
                    }
                }
            }
        }
    }

    let mut result = Vec::new();
    flatten(&value, &mut result);
    Ok(result)
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum DmapDecodeError {
    #[error("invalid tag")]
    InvalidTag,
    #[error("unexpected end of data")]
    UnexpectedEnd,
    #[error("invalid integer size: {0}")]
    InvalidIntSize(usize),
}
