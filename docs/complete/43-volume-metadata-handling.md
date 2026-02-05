# Section 43: Volume & Metadata Handling

## Dependencies
- **Section 36**: RTSP Server (SET_PARAMETER handling)
- **Section 37**: Session Management (session state)
- **Section 31**: DAAP Metadata (DMAP parsing)

## Overview

This section implements handling for:

1. **Volume Control**: Via RTSP SET_PARAMETER with volume values
2. **Track Metadata**: Artist, title, album from DMAP/DAAP format
3. **Album Artwork**: Cover images from RTSP
4. **Playback Progress**: Track position updates

These features enable rich display of "now playing" information on the receiver.

## Objectives

- Parse volume commands from SET_PARAMETER
- Map AirPlay dB volume to linear scale
- Parse DMAP-encoded track metadata
- Handle artwork (JPEG/PNG) delivery
- Parse playback progress updates
- Provide callbacks/events for UI integration

---

## Tasks

### 43.1 Volume Handling

- [x] **43.1.1** Parse and apply volume from SET_PARAMETER

**File:** `src/receiver/volume_handler.rs`

```rust
//! Volume handling for AirPlay receiver

use std::str::FromStr;

/// AirPlay volume range
/// -144.0 dB = silence
/// 0.0 dB = full volume
const VOLUME_MIN_DB: f32 = -144.0;
const VOLUME_MAX_DB: f32 = 0.0;

/// Volume update from SET_PARAMETER
#[derive(Debug, Clone, Copy)]
pub struct VolumeUpdate {
    /// Volume in dB (-144.0 to 0.0)
    pub db: f32,
    /// Muted (volume = -144)
    pub muted: bool,
    /// Linear volume (0.0 to 1.0)
    pub linear: f32,
}

impl VolumeUpdate {
    /// Create from dB value
    pub fn from_db(db: f32) -> Self {
        let db = db.clamp(VOLUME_MIN_DB, VOLUME_MAX_DB);
        let muted = db <= VOLUME_MIN_DB;
        let linear = db_to_linear(db);

        Self { db, muted, linear }
    }
}

/// Parse volume from SET_PARAMETER body
///
/// Format: "volume: -15.000000\r\n"
pub fn parse_volume_parameter(body: &str) -> Option<VolumeUpdate> {
    for line in body.lines() {
        let line = line.trim();

        if let Some(value_str) = line.strip_prefix("volume:") {
            let value_str = value_str.trim();

            if let Ok(db) = f32::from_str(value_str) {
                return Some(VolumeUpdate::from_db(db));
            }
        }
    }

    None
}

/// Convert dB volume to linear (0.0 to 1.0)
///
/// Uses power law: linear = 10^(dB/20)
pub fn db_to_linear(db: f32) -> f32 {
    if db <= VOLUME_MIN_DB {
        return 0.0;
    }
    if db >= VOLUME_MAX_DB {
        return 1.0;
    }

    // Standard dB to linear conversion
    10.0_f32.powf(db / 20.0)
}

/// Convert linear volume (0.0 to 1.0) to dB
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        return VOLUME_MIN_DB;
    }
    if linear >= 1.0 {
        return VOLUME_MAX_DB;
    }

    20.0 * linear.log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_volume() {
        let body = "volume: -15.000000\r\n";
        let update = parse_volume_parameter(body).unwrap();

        assert!((update.db - -15.0).abs() < 0.01);
        assert!(!update.muted);
    }

    #[test]
    fn test_parse_muted() {
        let body = "volume: -144.000000\r\n";
        let update = parse_volume_parameter(body).unwrap();

        assert!(update.muted);
        assert!(update.linear < 0.001);
    }

    #[test]
    fn test_db_to_linear() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 0.001);
        assert!((db_to_linear(-20.0) - 0.1).abs() < 0.01);
        assert!(db_to_linear(-144.0) < 0.001);
    }

    #[test]
    fn test_linear_to_db() {
        assert!((linear_to_db(1.0) - 0.0).abs() < 0.01);
        assert!((linear_to_db(0.1) - -20.0).abs() < 0.1);
    }
}
```

---

### 43.2 Metadata Parsing

- [x] **43.2.1** Parse DMAP-encoded track metadata

**File:** `src/receiver/metadata_handler.rs`

```rust
//! Track metadata handling for AirPlay receiver
//!
//! Parses DMAP (Digital Media Access Protocol) encoded metadata
//! from SET_PARAMETER requests.

use std::collections::HashMap;

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
mod dmap_tags {
    pub const ITEM_NAME: &[u8] = b"minm";        // Title
    pub const ITEM_ARTIST: &[u8] = b"asar";      // Artist
    pub const ITEM_ALBUM: &[u8] = b"asal";       // Album
    pub const ITEM_GENRE: &[u8] = b"asgn";       // Genre
    pub const TRACK_NUMBER: &[u8] = b"astn";     // Track number
    pub const TRACK_COUNT: &[u8] = b"astc";      // Track count
    pub const DISC_NUMBER: &[u8] = b"asdn";      // Disc number
    pub const DISC_COUNT: &[u8] = b"asdc";       // Disc count
    pub const DURATION: &[u8] = b"astm";         // Duration (ms)
}

/// Parse DMAP metadata from binary data
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

        if offset + length > data.len() {
            break;
        }

        let value = &data[offset..offset + length];
        offset += length;

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
                metadata.track_number = Some(u32::from_be_bytes([
                    value[0], value[1], value[2], value[3]
                ]));
            }
            t if t == dmap_tags::TRACK_COUNT && length >= 4 => {
                metadata.track_count = Some(u32::from_be_bytes([
                    value[0], value[1], value[2], value[3]
                ]));
            }
            t if t == dmap_tags::DISC_NUMBER && length >= 4 => {
                metadata.disc_number = Some(u32::from_be_bytes([
                    value[0], value[1], value[2], value[3]
                ]));
            }
            t if t == dmap_tags::DISC_COUNT && length >= 4 => {
                metadata.disc_count = Some(u32::from_be_bytes([
                    value[0], value[1], value[2], value[3]
                ]));
            }
            t if t == dmap_tags::DURATION && length >= 4 => {
                metadata.duration_ms = Some(u32::from_be_bytes([
                    value[0], value[1], value[2], value[3]
                ]));
            }
            _ => {
                // Unknown tag, skip
            }
        }
    }

    Ok(metadata)
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("Invalid DMAP format")]
    InvalidFormat,

    #[error("Incomplete data")]
    IncompleteData,
}
```

---

### 43.3 Artwork Handling

- [x] **43.3.1** Parse and store album artwork

**File:** `src/receiver/artwork_handler.rs`

```rust
//! Album artwork handling

/// Album artwork
#[derive(Debug, Clone)]
pub struct Artwork {
    /// Image data (JPEG or PNG)
    pub data: Vec<u8>,
    /// MIME type
    pub mime_type: String,
    /// Width (if known)
    pub width: Option<u32>,
    /// Height (if known)
    pub height: Option<u32>,
}

impl Artwork {
    /// Create from raw image data
    pub fn from_data(data: Vec<u8>) -> Option<Self> {
        let mime_type = detect_image_type(&data)?;

        Some(Self {
            data,
            mime_type,
            width: None,
            height: None,
        })
    }

    /// Check if artwork is JPEG
    pub fn is_jpeg(&self) -> bool {
        self.mime_type == "image/jpeg"
    }

    /// Check if artwork is PNG
    pub fn is_png(&self) -> bool {
        self.mime_type == "image/png"
    }
}

/// Detect image type from magic bytes
fn detect_image_type(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }

    // JPEG: starts with FF D8 FF
    if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return Some("image/jpeg".to_string());
    }

    // PNG: starts with 89 50 4E 47
    if data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
        return Some("image/png".to_string());
    }

    None
}

/// Parse artwork from SET_PARAMETER body
pub fn parse_artwork(content_type: &str, data: &[u8]) -> Option<Artwork> {
    if content_type.contains("image/jpeg") || content_type.contains("image/png") {
        Artwork::from_data(data.to_vec())
    } else {
        None
    }
}
```

---

### 43.4 Progress Updates

- [x] **43.4.1** Parse playback progress

**File:** `src/receiver/progress_handler.rs`

```rust
//! Playback progress handling

use std::time::Duration;

/// Playback progress update
#[derive(Debug, Clone, Copy)]
pub struct PlaybackProgress {
    /// Start position in seconds
    pub start: f64,
    /// Current position in seconds
    pub current: f64,
    /// End position (duration) in seconds
    pub end: f64,
}

impl PlaybackProgress {
    /// Get current position as Duration
    pub fn position(&self) -> Duration {
        Duration::from_secs_f64(self.current)
    }

    /// Get total duration
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.end)
    }

    /// Get progress as percentage (0.0 to 1.0)
    pub fn percentage(&self) -> f64 {
        if self.end <= 0.0 {
            return 0.0;
        }
        (self.current / self.end).clamp(0.0, 1.0)
    }

    /// Get remaining time
    pub fn remaining(&self) -> Duration {
        Duration::from_secs_f64((self.end - self.current).max(0.0))
    }
}

/// Parse progress from SET_PARAMETER body
///
/// Format: "progress: start/current/end\r\n"
/// Values are in seconds (can be floating point)
pub fn parse_progress(body: &str) -> Option<PlaybackProgress> {
    for line in body.lines() {
        let line = line.trim();

        if let Some(value) = line.strip_prefix("progress:") {
            let parts: Vec<&str> = value.trim().split('/').collect();

            if parts.len() == 3 {
                let start: f64 = parts[0].parse().ok()?;
                let current: f64 = parts[1].parse().ok()?;
                let end: f64 = parts[2].parse().ok()?;

                return Some(PlaybackProgress { start, current, end });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_progress() {
        let body = "progress: 0.0/30.5/180.0\r\n";
        let progress = parse_progress(body).unwrap();

        assert!((progress.start - 0.0).abs() < 0.01);
        assert!((progress.current - 30.5).abs() < 0.01);
        assert!((progress.end - 180.0).abs() < 0.01);
    }

    #[test]
    fn test_progress_percentage() {
        let progress = PlaybackProgress {
            start: 0.0,
            current: 60.0,
            end: 120.0,
        };

        assert!((progress.percentage() - 0.5).abs() < 0.01);
    }
}
```

---

### 43.5 SET_PARAMETER Router

- [x] **43.5.1** Route SET_PARAMETER to appropriate handler

**File:** `src/receiver/set_parameter_handler.rs`

```rust
//! SET_PARAMETER request routing

use crate::protocol::rtsp::RtspRequest;
use super::volume_handler::{parse_volume_parameter, VolumeUpdate};
use super::metadata_handler::{parse_dmap_metadata, TrackMetadata};
use super::artwork_handler::{parse_artwork, Artwork};
use super::progress_handler::{parse_progress, PlaybackProgress};

/// Result of processing SET_PARAMETER
#[derive(Debug)]
pub enum ParameterUpdate {
    Volume(VolumeUpdate),
    Metadata(TrackMetadata),
    Artwork(Artwork),
    Progress(PlaybackProgress),
    Unknown(String),
}

/// Process SET_PARAMETER request
pub fn process_set_parameter(request: &RtspRequest) -> Vec<ParameterUpdate> {
    let mut updates = Vec::new();

    let content_type = request.headers.get("Content-Type")
        .map(|s| s.as_str())
        .unwrap_or("");

    let body = &request.body;
    let body_str = String::from_utf8_lossy(body);

    // Route based on content type
    if content_type.contains("text/parameters") {
        // Text parameters (volume, progress)
        if let Some(volume) = parse_volume_parameter(&body_str) {
            updates.push(ParameterUpdate::Volume(volume));
        }

        if let Some(progress) = parse_progress(&body_str) {
            updates.push(ParameterUpdate::Progress(progress));
        }
    } else if content_type.contains("application/x-dmap-tagged") {
        // DMAP metadata
        if let Ok(metadata) = parse_dmap_metadata(body) {
            updates.push(ParameterUpdate::Metadata(metadata));
        }
    } else if content_type.contains("image/") {
        // Artwork
        if let Some(artwork) = parse_artwork(content_type, body) {
            updates.push(ParameterUpdate::Artwork(artwork));
        }
    } else if !content_type.is_empty() {
        updates.push(ParameterUpdate::Unknown(content_type.to_string()));
    }

    updates
}
```

---

## Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dmap_metadata_parsing() {
        // Construct minimal DMAP data
        // "minm" + length(5) + "Hello"
        let mut data = Vec::new();
        data.extend_from_slice(b"minm");
        data.extend_from_slice(&5u32.to_be_bytes());
        data.extend_from_slice(b"Hello");

        let metadata = parse_dmap_metadata(&data).unwrap();
        assert_eq!(metadata.title, Some("Hello".to_string()));
    }

    #[test]
    fn test_artwork_detection() {
        // JPEG magic bytes
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let artwork = Artwork::from_data(jpeg_data).unwrap();
        assert!(artwork.is_jpeg());

        // PNG magic bytes
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A];
        let artwork = Artwork::from_data(png_data).unwrap();
        assert!(artwork.is_png());
    }
}
```

---

## Acceptance Criteria

- [x] Volume parsing works for all valid dB values
- [x] dB to linear conversion accurate
- [x] DMAP metadata parsing extracts all fields
- [x] Artwork detection works for JPEG and PNG
- [x] Progress parsing works with floating point values
- [x] SET_PARAMETER routing selects correct handler
- [x] All unit tests pass

---

## Notes

- **Volume range**: -144 to 0 dB (AirPlay standard)
- **Metadata**: Not all fields always present; handle gracefully
- **Artwork**: Can be large (several MB); consider memory management
- **Progress**: Updates may come frequently; debounce for UI
- **Password**: Future section will add authentication hooks

---

## References

- [DMAP Protocol](https://daap.sourceforge.net/)
- [AirPlay Volume](https://nto.github.io/AirPlay.html#audio-volume)
