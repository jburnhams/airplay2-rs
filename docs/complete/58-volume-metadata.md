# Section 58: Volume, Metadata & Artwork

## Dependencies
- **Section 48**: RTSP/HTTP Server Extensions
- **Section 29**: DAAP Metadata Codec (reuse from client)
- **Section 43**: Volume & Metadata Handling (AirPlay 1 patterns)

## Overview

This section handles SET_PARAMETER requests for volume control, track metadata (title, artist, album), and artwork. These use the same formats as AirPlay 1 but may be delivered in encrypted form after pairing.

## Objectives

- Handle volume control (dB scale, -144 to 0)
- Parse DMAP/DAAP metadata
- Receive and store artwork
- Emit events for UI updates
- Reuse existing DAAP codec

---

## Tasks

### 58.1 Volume Handler

**File:** `src/receiver/ap2/volume_handler.rs`

```rust
//! Volume Control for AirPlay 2 Receiver

use super::request_handler::{Ap2HandleResult, Ap2Event, Ap2RequestContext};
use super::response_builder::Ap2ResponseBuilder;
use super::body_handler::parse_text_parameters;
use crate::protocol::rtsp::{RtspRequest, StatusCode};
use std::sync::{Arc, RwLock};

/// Volume controller
pub struct VolumeController {
    /// Current volume in dB (-144 to 0)
    volume_db: Arc<RwLock<f32>>,
    /// Muted flag
    muted: Arc<RwLock<bool>>,
}

impl VolumeController {
    pub fn new() -> Self {
        Self {
            volume_db: Arc::new(RwLock::new(-20.0)),  // Default -20dB
            muted: Arc::new(RwLock::new(false)),
        }
    }

    /// Set volume in dB (clamped to -144..0)
    pub fn set_volume_db(&self, volume: f32) {
        let clamped = volume.clamp(-144.0, 0.0);
        *self.volume_db.write().unwrap() = clamped;
        log::debug!("Volume set to {:.1} dB", clamped);
    }

    /// Get current volume in dB
    pub fn volume_db(&self) -> f32 {
        *self.volume_db.read().unwrap()
    }

    /// Convert dB to linear (0.0 to 1.0)
    pub fn volume_linear(&self) -> f32 {
        let db = self.volume_db();
        if db <= -144.0 {
            0.0
        } else {
            10.0_f32.powf(db / 20.0)
        }
    }

    /// Set muted state
    pub fn set_muted(&self, muted: bool) {
        *self.muted.write().unwrap() = muted;
    }

    /// Check if muted
    pub fn is_muted(&self) -> bool {
        *self.muted.read().unwrap()
    }

    /// Handle SET_PARAMETER with volume
    pub fn handle_set_volume(&self, body: &[u8]) -> Result<f32, VolumeError> {
        let params = parse_text_parameters(body)
            .map_err(|_| VolumeError::ParseError)?;

        let volume_str = params.get("volume")
            .ok_or(VolumeError::MissingVolume)?;

        let volume: f32 = volume_str.parse()
            .map_err(|_| VolumeError::InvalidValue)?;

        self.set_volume_db(volume);
        Ok(volume)
    }
}

impl Default for VolumeController {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VolumeError {
    #[error("Failed to parse volume parameters")]
    ParseError,
    #[error("Missing volume parameter")]
    MissingVolume,
    #[error("Invalid volume value")]
    InvalidValue,
}

/// Handle SET_PARAMETER for volume
pub fn handle_volume_set_parameter(
    request: &RtspRequest,
    cseq: u32,
    controller: &VolumeController,
) -> Ap2HandleResult {
    match controller.handle_set_volume(&request.body) {
        Ok(volume) => Ap2HandleResult {
            response: Ap2ResponseBuilder::ok().cseq(cseq).encode(),
            new_state: None,
            event: Some(Ap2Event::VolumeChanged { volume }),
            error: None,
        },
        Err(e) => Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode::BAD_REQUEST)
                .cseq(cseq).encode(),
            new_state: None,
            event: None,
            error: Some(e.to_string()),
        },
    }
}
```

---

### 58.2 Metadata Handler

**File:** `src/receiver/ap2/metadata_handler.rs`

```rust
//! Metadata and Artwork Handling

use crate::protocol::daap::{DmapParser, DmapValue};
use std::sync::{Arc, RwLock};

/// Track metadata
#[derive(Debug, Clone, Default)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub duration_ms: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
}

/// Artwork data
#[derive(Debug, Clone)]
pub struct Artwork {
    pub data: Vec<u8>,
    pub mime_type: String,
}

/// Metadata controller
pub struct MetadataController {
    metadata: Arc<RwLock<TrackMetadata>>,
    artwork: Arc<RwLock<Option<Artwork>>>,
}

impl MetadataController {
    pub fn new() -> Self {
        Self {
            metadata: Arc::new(RwLock::new(TrackMetadata::default())),
            artwork: Arc::new(RwLock::new(None)),
        }
    }

    /// Parse and update metadata from DMAP data
    pub fn update_metadata(&self, dmap_data: &[u8]) -> Result<(), MetadataError> {
        let parsed = DmapParser::parse(dmap_data)
            .map_err(|e| MetadataError::ParseError(e.to_string()))?;

        let mut metadata = self.metadata.write().unwrap();

        // Extract known fields
        if let Some(title) = Self::get_string(&parsed, "minm") {
            metadata.title = Some(title);
        }
        if let Some(artist) = Self::get_string(&parsed, "asar") {
            metadata.artist = Some(artist);
        }
        if let Some(album) = Self::get_string(&parsed, "asal") {
            metadata.album = Some(album);
        }
        if let Some(genre) = Self::get_string(&parsed, "asgn") {
            metadata.genre = Some(genre);
        }

        log::debug!("Metadata updated: {:?}", *metadata);
        Ok(())
    }

    /// Update artwork
    pub fn update_artwork(&self, data: Vec<u8>, mime_type: String) {
        *self.artwork.write().unwrap() = Some(Artwork { data, mime_type });
        log::debug!("Artwork updated ({} bytes)", self.artwork.read().unwrap().as_ref().map(|a| a.data.len()).unwrap_or(0));
    }

    /// Get current metadata
    pub fn metadata(&self) -> TrackMetadata {
        self.metadata.read().unwrap().clone()
    }

    /// Get current artwork
    pub fn artwork(&self) -> Option<Artwork> {
        self.artwork.read().unwrap().clone()
    }

    /// Clear metadata and artwork
    pub fn clear(&self) {
        *self.metadata.write().unwrap() = TrackMetadata::default();
        *self.artwork.write().unwrap() = None;
    }

    fn get_string(dmap: &DmapValue, key: &str) -> Option<String> {
        // Navigate DMAP structure to find key
        if let DmapValue::Container(items) = dmap {
            for (k, v) in items {
                if k == key {
                    if let DmapValue::String(s) = v {
                        return Some(s.clone());
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

#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("Failed to parse DMAP: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
```

---

## Acceptance Criteria

 - [x] Volume parsing from SET_PARAMETER
 - [x] dB to linear conversion
 - [x] DMAP metadata parsing
 - [x] Artwork storage
 - [x] Events emitted on changes
 - [x] Reuses existing DAAP codec
 - [x] All unit tests pass

---

## References

- [DAAP/DMAP Protocol](https://en.wikipedia.org/wiki/Digital_Audio_Access_Protocol)
- [Section 29: DAAP Metadata Codec](./complete/29-daap-metadata-codec.md)
