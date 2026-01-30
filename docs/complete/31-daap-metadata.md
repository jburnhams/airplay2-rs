# Section 31: DAAP/DMAP Metadata Protocol

> **VERIFIED**: Protocol documentation for metadata transmission.
> SET_PARAMETER with metadata content types supported. Checked 2025-01-30.

## Dependencies
- **Section 27**: RTSP Session for RAOP (must be complete)
- **Section 03**: Binary Plist Codec (helpful for understanding encoding)

## Overview

RAOP supports transmitting track metadata (title, artist, album), artwork (cover images), and playback progress to AirPlay receivers. This uses a subset of DAAP (Digital Audio Access Protocol) with DMAP (Digital Media Access Protocol) encoding.

Metadata is sent via RTSP `SET_PARAMETER` requests with specific content types:
- **Track info**: `application/x-dmap-tagged` (DAAP format)
- **Artwork**: `image/jpeg` or `image/png`
- **Progress**: `text/parameters` (plain text)

## Protocol Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    Metadata Transmission                         │
│                                                                  │
│  Client                                           Server         │
│    │                                                │            │
│    │  SET_PARAMETER (track info)                   │            │
│    │  Content-Type: application/x-dmap-tagged      │            │
│    │  RTP-Info: rtptime={timestamp}                │            │
│    │─────────────────────────────────────────────>│            │
│    │                                                │            │
│    │  SET_PARAMETER (artwork)                      │            │
│    │  Content-Type: image/jpeg                     │            │
│    │  RTP-Info: rtptime={timestamp}                │            │
│    │─────────────────────────────────────────────>│            │
│    │                                                │            │
│    │  SET_PARAMETER (progress)                     │            │
│    │  Content-Type: text/parameters                │            │
│    │─────────────────────────────────────────────>│            │
│    │                                                │            │
└─────────────────────────────────────────────────────────────────┘
```

## Objectives

- Implement DMAP encoding for track metadata
- Support JPEG/PNG artwork transmission
- Implement progress reporting
- Integrate with RTSP session management

---

## Tasks

### 31.1 DMAP Encoding

- [x] **31.1.1** Implement DMAP types and encoder

**File:** `src/protocol/daap/mod.rs`

```rust
//! DAAP/DMAP metadata protocol for RAOP

mod dmap;
mod metadata;
mod artwork;
mod progress;

pub use dmap::{DmapEncoder, DmapTag};
pub use metadata::{TrackMetadata, MetadataBuilder};
pub use artwork::{Artwork, ArtworkFormat};
pub use progress::PlaybackProgress;
```

**File:** `src/protocol/daap/dmap.rs`

```rust
//! DMAP (Digital Media Access Protocol) encoding

use std::io::Write;

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
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
        }
    }

    /// Encode a tag-value pair
    pub fn encode_tag(&mut self, tag: DmapTag, value: &DmapValue) {
        // Write 4-byte tag code
        self.buffer.extend_from_slice(tag.code());

        match value {
            DmapValue::String(s) => {
                // Write length (4 bytes, big-endian)
                let len = s.len() as u32;
                self.buffer.extend_from_slice(&len.to_be_bytes());
                // Write string bytes
                self.buffer.extend_from_slice(s.as_bytes());
            }
            DmapValue::Int(n) => {
                // Determine appropriate size
                if *n >= 0 && *n <= 255 {
                    self.buffer.extend_from_slice(&1u32.to_be_bytes());
                    self.buffer.push(*n as u8);
                } else if *n >= i16::MIN as i64 && *n <= i16::MAX as i64 {
                    self.buffer.extend_from_slice(&2u32.to_be_bytes());
                    self.buffer.extend_from_slice(&(*n as i16).to_be_bytes());
                } else if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                    self.buffer.extend_from_slice(&4u32.to_be_bytes());
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
                let len = inner_data.len() as u32;
                self.buffer.extend_from_slice(&len.to_be_bytes());
                self.buffer.extend_from_slice(&inner_data);
            }
            DmapValue::Raw(data) => {
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
pub fn decode_dmap(data: &[u8]) -> Result<Vec<(String, String)>, DmapDecodeError> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos + 8 <= data.len() {
        let tag = std::str::from_utf8(&data[pos..pos + 4])
            .map_err(|_| DmapDecodeError::InvalidTag)?;
        let len = u32::from_be_bytes([
            data[pos + 4],
            data[pos + 5],
            data[pos + 6],
            data[pos + 7],
        ]) as usize;

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
pub enum DmapDecodeError {
    #[error("invalid tag")]
    InvalidTag,
    #[error("unexpected end of data")]
    UnexpectedEnd,
}
```

---

### 31.2 Track Metadata

- [x] **31.2.1** Implement track metadata encoding

**File:** `src/protocol/daap/metadata.rs`

```rust
//! Track metadata for RAOP

use super::dmap::{DmapEncoder, DmapTag};

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
    pub fn new() -> Self {
        Self::default()
    }

    /// Create builder
    pub fn builder() -> MetadataBuilder {
        MetadataBuilder::new()
    }

    /// Encode as DMAP for SET_PARAMETER
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
            item_encoder.int(DmapTag::SongTrackNumber, track as i64);
        }

        if let Some(disc) = self.disc_number {
            item_encoder.int(DmapTag::SongDiscNumber, disc as i64);
        }

        if let Some(year) = self.year {
            item_encoder.int(DmapTag::SongYear, year as i64);
        }

        if let Some(duration) = self.duration_ms {
            item_encoder.int(DmapTag::SongTime, duration as i64);
        }

        item_encoder.finish()
    }

    /// Check if metadata is empty
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
    pub fn new() -> Self {
        Self {
            metadata: TrackMetadata::new(),
        }
    }

    /// Set title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.metadata.title = Some(title.into());
        self
    }

    /// Set artist
    pub fn artist(mut self, artist: impl Into<String>) -> Self {
        self.metadata.artist = Some(artist.into());
        self
    }

    /// Set album
    pub fn album(mut self, album: impl Into<String>) -> Self {
        self.metadata.album = Some(album.into());
        self
    }

    /// Set genre
    pub fn genre(mut self, genre: impl Into<String>) -> Self {
        self.metadata.genre = Some(genre.into());
        self
    }

    /// Set track number
    pub fn track_number(mut self, track: u32) -> Self {
        self.metadata.track_number = Some(track);
        self
    }

    /// Set disc number
    pub fn disc_number(mut self, disc: u32) -> Self {
        self.metadata.disc_number = Some(disc);
        self
    }

    /// Set year
    pub fn year(mut self, year: u32) -> Self {
        self.metadata.year = Some(year);
        self
    }

    /// Set duration in milliseconds
    pub fn duration_ms(mut self, duration: u32) -> Self {
        self.metadata.duration_ms = Some(duration);
        self
    }

    /// Build the metadata
    pub fn build(self) -> TrackMetadata {
        self.metadata
    }
}

impl Default for MetadataBuilder {
    fn default() -> Self {
        Self::new()
    }
}
```

---

### 31.3 Artwork

- [x] **31.3.1** Implement artwork transmission

**File:** `src/protocol/daap/artwork.rs`

```rust
//! Album artwork for RAOP

/// Artwork image format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkFormat {
    /// JPEG image
    Jpeg,
    /// PNG image
    Png,
}

impl ArtworkFormat {
    /// Get MIME type
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
        }
    }

    /// Detect format from data
    pub fn detect(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        // JPEG magic bytes
        if data[0..2] == [0xFF, 0xD8] {
            return Some(Self::Jpeg);
        }

        // PNG magic bytes
        if data[0..4] == [0x89, 0x50, 0x4E, 0x47] {
            return Some(Self::Png);
        }

        None
    }
}

/// Album artwork
#[derive(Debug, Clone)]
pub struct Artwork {
    /// Image data
    pub data: Vec<u8>,
    /// Image format
    pub format: ArtworkFormat,
}

impl Artwork {
    /// Create artwork from JPEG data
    pub fn jpeg(data: Vec<u8>) -> Self {
        Self {
            data,
            format: ArtworkFormat::Jpeg,
        }
    }

    /// Create artwork from PNG data
    pub fn png(data: Vec<u8>) -> Self {
        Self {
            data,
            format: ArtworkFormat::Png,
        }
    }

    /// Create artwork with auto-detected format
    pub fn from_data(data: Vec<u8>) -> Option<Self> {
        let format = ArtworkFormat::detect(&data)?;
        Some(Self { data, format })
    }

    /// Get MIME type for Content-Type header
    pub fn mime_type(&self) -> &'static str {
        self.format.mime_type()
    }

    /// Get image dimensions (basic parsing)
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        match self.format {
            ArtworkFormat::Jpeg => self.jpeg_dimensions(),
            ArtworkFormat::Png => self.png_dimensions(),
        }
    }

    fn jpeg_dimensions(&self) -> Option<(u32, u32)> {
        // Simple JPEG dimension parser
        let mut pos = 2;

        while pos < self.data.len() - 4 {
            if self.data[pos] != 0xFF {
                pos += 1;
                continue;
            }

            let marker = self.data[pos + 1];

            // SOF markers contain dimensions
            if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
                if pos + 9 < self.data.len() {
                    let height = u16::from_be_bytes([
                        self.data[pos + 5],
                        self.data[pos + 6],
                    ]) as u32;
                    let width = u16::from_be_bytes([
                        self.data[pos + 7],
                        self.data[pos + 8],
                    ]) as u32;
                    return Some((width, height));
                }
            }

            // Skip to next marker
            if pos + 3 < self.data.len() {
                let len = u16::from_be_bytes([
                    self.data[pos + 2],
                    self.data[pos + 3],
                ]) as usize;
                pos += 2 + len;
            } else {
                break;
            }
        }

        None
    }

    fn png_dimensions(&self) -> Option<(u32, u32)> {
        // PNG IHDR chunk contains dimensions at bytes 16-23
        if self.data.len() < 24 {
            return None;
        }

        let width = u32::from_be_bytes([
            self.data[16],
            self.data[17],
            self.data[18],
            self.data[19],
        ]);
        let height = u32::from_be_bytes([
            self.data[20],
            self.data[21],
            self.data[22],
            self.data[23],
        ]);

        Some((width, height))
    }
}
```

---

### 31.4 Progress Reporting

- [x] **31.4.1** Implement progress updates

**File:** `src/protocol/daap/progress.rs`

```rust
//! Playback progress for RAOP

/// Playback progress information
#[derive(Debug, Clone, Copy)]
pub struct PlaybackProgress {
    /// RTP timestamp of track start
    pub start: u32,
    /// RTP timestamp of current position
    pub current: u32,
    /// RTP timestamp of track end
    pub end: u32,
}

impl PlaybackProgress {
    /// Create new progress
    pub fn new(start: u32, current: u32, end: u32) -> Self {
        Self { start, current, end }
    }

    /// Create progress for track at given position
    ///
    /// # Arguments
    /// * `base_timestamp` - RTP timestamp at track start
    /// * `position_samples` - Current position in samples
    /// * `duration_samples` - Total duration in samples
    pub fn from_samples(
        base_timestamp: u32,
        position_samples: u32,
        duration_samples: u32,
    ) -> Self {
        Self {
            start: base_timestamp,
            current: base_timestamp.wrapping_add(position_samples),
            end: base_timestamp.wrapping_add(duration_samples),
        }
    }

    /// Encode as text/parameters body
    pub fn encode(&self) -> String {
        format!("progress: {}/{}/{}\r\n", self.start, self.current, self.end)
    }

    /// Get current position in seconds (at 44.1kHz)
    pub fn position_secs(&self) -> f64 {
        let samples = self.current.wrapping_sub(self.start);
        samples as f64 / 44100.0
    }

    /// Get duration in seconds (at 44.1kHz)
    pub fn duration_secs(&self) -> f64 {
        let samples = self.end.wrapping_sub(self.start);
        samples as f64 / 44100.0
    }

    /// Get progress as percentage (0.0 - 1.0)
    pub fn percentage(&self) -> f64 {
        let total = self.end.wrapping_sub(self.start) as f64;
        if total == 0.0 {
            return 0.0;
        }
        let current = self.current.wrapping_sub(self.start) as f64;
        (current / total).clamp(0.0, 1.0)
    }

    /// Parse from text/parameters body
    pub fn parse(text: &str) -> Option<Self> {
        let line = text.lines().find(|l| l.starts_with("progress:"))?;
        let values = line.strip_prefix("progress:")?.trim();
        let parts: Vec<&str> = values.split('/').collect();

        if parts.len() != 3 {
            return None;
        }

        Some(Self {
            start: parts[0].trim().parse().ok()?,
            current: parts[1].trim().parse().ok()?,
            end: parts[2].trim().parse().ok()?,
        })
    }
}
```

---

### 31.5 Metadata RTSP Integration

- [x] **31.5.1** Add metadata methods to RAOP session

**File:** `src/protocol/raop/session.rs` (additions)

```rust
impl RaopRtspSession {
    /// Send track metadata
    pub fn set_metadata_request(
        &mut self,
        metadata: &TrackMetadata,
        rtptime: u32,
    ) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::SetParameter, self.uri(""));

        let body = metadata.encode_dmap();

        self.add_common_headers(builder, cseq)
            .header(names::CONTENT_TYPE, "application/x-dmap-tagged")
            .header("RTP-Info", format!("rtptime={}", rtptime))
            .body(body)
            .build()
    }

    /// Send artwork
    pub fn set_artwork_request(
        &mut self,
        artwork: &Artwork,
        rtptime: u32,
    ) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::SetParameter, self.uri(""));

        self.add_common_headers(builder, cseq)
            .header(names::CONTENT_TYPE, artwork.mime_type())
            .header("RTP-Info", format!("rtptime={}", rtptime))
            .body(artwork.data.clone())
            .build()
    }

    /// Send progress update
    pub fn set_progress_request(
        &mut self,
        progress: &PlaybackProgress,
    ) -> RtspRequest {
        let cseq = self.next_cseq();
        let builder = RtspRequest::builder(Method::SetParameter, self.uri(""));

        self.add_common_headers(builder, cseq)
            .header(names::CONTENT_TYPE, "text/parameters")
            .body(progress.encode().into_bytes())
            .build()
    }
}
```

---

## Unit Tests

### Test File: `src/protocol/daap/dmap.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_string() {
        let mut encoder = DmapEncoder::new();
        encoder.string(DmapTag::ItemName, "Test Song");

        let data = encoder.finish();

        // Tag (4) + Length (4) + "Test Song" (9) = 17 bytes
        assert_eq!(data.len(), 17);
        assert_eq!(&data[0..4], b"minm");
        assert_eq!(u32::from_be_bytes([data[4], data[5], data[6], data[7]]), 9);
        assert_eq!(&data[8..], b"Test Song");
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut encoder = DmapEncoder::new();
        encoder.string(DmapTag::ItemName, "My Track");
        encoder.string(DmapTag::SongArtist, "Artist Name");

        let data = encoder.finish();
        let decoded = decode_dmap(&data).unwrap();

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].0, "minm");
        assert_eq!(decoded[0].1, "My Track");
        assert_eq!(decoded[1].0, "asar");
        assert_eq!(decoded[1].1, "Artist Name");
    }
}
```

### Test File: `src/protocol/daap/progress.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_encode() {
        let progress = PlaybackProgress::new(0, 44100, 441000);
        let encoded = progress.encode();

        assert_eq!(encoded, "progress: 0/44100/441000\r\n");
    }

    #[test]
    fn test_progress_parse() {
        let text = "progress: 1000/2000/3000\r\n";
        let progress = PlaybackProgress::parse(text).unwrap();

        assert_eq!(progress.start, 1000);
        assert_eq!(progress.current, 2000);
        assert_eq!(progress.end, 3000);
    }

    #[test]
    fn test_progress_percentage() {
        let progress = PlaybackProgress::new(0, 50, 100);
        assert!((progress.percentage() - 0.5).abs() < 0.001);

        let progress = PlaybackProgress::new(0, 0, 100);
        assert!((progress.percentage() - 0.0).abs() < 0.001);

        let progress = PlaybackProgress::new(0, 100, 100);
        assert!((progress.percentage() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_progress_from_samples() {
        // 10 seconds at 44.1kHz
        let progress = PlaybackProgress::from_samples(1000, 441000, 4410000);

        assert_eq!(progress.start, 1000);
        assert_eq!(progress.current, 442000);
        assert_eq!(progress.end, 4411000);
    }
}
```

---

## Acceptance Criteria

- [x] DMAP encoding produces valid output
- [x] Track metadata includes all relevant fields
- [x] Artwork format detection works correctly
- [x] Progress encoding/parsing is correct
- [x] RTSP integration uses correct content types
- [x] RTP-Info header includes timestamp
- [x] All unit tests pass
- [x] Integration tests pass

---

## Notes

- Metadata should be sent before or at the start of playback
- Progress updates can be sent periodically (every second)
- Artwork should be reasonably sized (typically 500x500 or less)
- Some receivers may ignore certain metadata fields
- DMAP encoding is big-endian for integers
