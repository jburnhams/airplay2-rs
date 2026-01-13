# Section 14: URL-Based Streaming

## Dependencies
- **Section 05**: RTSP Protocol (must be complete)
- **Section 10**: Connection Management (must be complete)
- **Section 13**: PCM Streaming (for audio handling concepts)

## Overview

AirPlay supports URL-based streaming where the device fetches and plays audio/video from a URL. This is more efficient for:
- Streaming from cloud services
- Playing internet radio
- Playing podcasts/audiobooks

## Objectives

- Implement URL playback initiation
- Handle playback info updates
- Support scrubbing/seeking
- Handle playback events from device

---

## Tasks

### 14.1 URL Playback

- [ ] **14.1.1** Implement URL streaming

**File:** `src/streaming/url.rs`

```rust
//! URL-based streaming for AirPlay

use crate::connection::ConnectionManager;
use crate::protocol::rtsp::{Method, RtspRequest};
use crate::protocol::plist::PlistValue;
use crate::error::AirPlayError;

use std::sync::Arc;
use std::time::Duration;

/// URL streaming session
pub struct UrlStreamer {
    /// Connection manager
    connection: Arc<ConnectionManager>,
    /// Current URL being played
    current_url: Option<String>,
    /// Playback info
    playback_info: Option<PlaybackInfo>,
}

/// Playback information
#[derive(Debug, Clone)]
pub struct PlaybackInfo {
    /// Current position in seconds
    pub position: f64,
    /// Total duration in seconds
    pub duration: f64,
    /// Playback rate (1.0 = normal, 0.0 = paused)
    pub rate: f32,
    /// Is currently playing
    pub playing: bool,
    /// Ready to play
    pub ready_to_play: bool,
    /// Playback buffer state
    pub playback_buffer_empty: bool,
    /// Loaded time ranges
    pub loaded_time_ranges: Vec<TimeRange>,
    /// Seekable time ranges
    pub seekable_time_ranges: Vec<TimeRange>,
}

/// Time range
#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start: f64,
    pub duration: f64,
}

impl UrlStreamer {
    /// Create a new URL streamer
    pub fn new(connection: Arc<ConnectionManager>) -> Self {
        Self {
            connection,
            current_url: None,
            playback_info: None,
        }
    }

    /// Start playing a URL
    pub async fn play(&mut self, url: &str) -> Result<(), AirPlayError> {
        // Build the play request body as plist
        let body = PlistValue::Dictionary(vec![
            ("Content-Location".to_string(), PlistValue::String(url.to_string())),
            ("Start-Position".to_string(), PlistValue::Real(0.0)),
        ]);

        // Send PLAY command
        self.send_command(Method::Play, Some(body)).await?;

        self.current_url = Some(url.to_string());
        Ok(())
    }

    /// Start playing at a specific position
    pub async fn play_at(&mut self, url: &str, position: f64) -> Result<(), AirPlayError> {
        let body = PlistValue::Dictionary(vec![
            ("Content-Location".to_string(), PlistValue::String(url.to_string())),
            ("Start-Position".to_string(), PlistValue::Real(position)),
        ]);

        self.send_command(Method::Play, Some(body)).await?;

        self.current_url = Some(url.to_string());
        Ok(())
    }

    /// Get current playback info
    pub async fn get_playback_info(&mut self) -> Result<PlaybackInfo, AirPlayError> {
        let response = self.send_command(Method::GetParameter, None).await?;

        // Parse response body as plist
        let info = self.parse_playback_info(&response)?;
        self.playback_info = Some(info.clone());

        Ok(info)
    }

    /// Scrub (seek) to position
    pub async fn scrub(&self, position: f64) -> Result<(), AirPlayError> {
        let body = PlistValue::Dictionary(vec![
            ("position".to_string(), PlistValue::Real(position)),
        ]);

        self.send_command(Method::SetParameter, Some(body)).await?;
        Ok(())
    }

    /// Set playback rate (0.0 = pause, 1.0 = play)
    pub async fn set_rate(&self, rate: f32) -> Result<(), AirPlayError> {
        let body = PlistValue::Dictionary(vec![
            ("rate".to_string(), PlistValue::Real(rate as f64)),
        ]);

        self.send_command(Method::SetParameter, Some(body)).await?;
        Ok(())
    }

    /// Pause playback
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        self.set_rate(0.0).await
    }

    /// Resume playback
    pub async fn resume(&self) -> Result<(), AirPlayError> {
        self.set_rate(1.0).await
    }

    /// Stop playback
    pub async fn stop(&mut self) -> Result<(), AirPlayError> {
        self.send_command(Method::Teardown, None).await?;
        self.current_url = None;
        self.playback_info = None;
        Ok(())
    }

    /// Get current position
    pub fn position(&self) -> Option<Duration> {
        self.playback_info.as_ref().map(|info| {
            Duration::from_secs_f64(info.position)
        })
    }

    /// Get total duration
    pub fn duration(&self) -> Option<Duration> {
        self.playback_info.as_ref().map(|info| {
            Duration::from_secs_f64(info.duration)
        })
    }

    /// Check if playing
    pub fn is_playing(&self) -> bool {
        self.playback_info.as_ref()
            .map(|info| info.playing)
            .unwrap_or(false)
    }

    /// Send RTSP command
    async fn send_command(
        &self,
        method: Method,
        body: Option<PlistValue>,
    ) -> Result<Vec<u8>, AirPlayError> {
        // TODO: Implement actual RTSP communication
        // This is a placeholder
        Ok(Vec::new())
    }

    /// Parse playback info from response
    fn parse_playback_info(&self, data: &[u8]) -> Result<PlaybackInfo, AirPlayError> {
        // Parse plist response
        let plist = crate::protocol::plist::decode(data)
            .map_err(|e| AirPlayError::ProtocolError {
                message: format!("Failed to parse playback info: {}", e),
            })?;

        // Extract fields from plist
        let info = if let PlistValue::Dictionary(dict) = plist {
            let get_f64 = |key: &str| -> f64 {
                dict.iter()
                    .find(|(k, _)| k == key)
                    .and_then(|(_, v)| match v {
                        PlistValue::Real(f) => Some(*f),
                        PlistValue::Integer(i) => Some(*i as f64),
                        _ => None,
                    })
                    .unwrap_or(0.0)
            };

            let get_bool = |key: &str| -> bool {
                dict.iter()
                    .find(|(k, _)| k == key)
                    .and_then(|(_, v)| match v {
                        PlistValue::Boolean(b) => Some(*b),
                        _ => None,
                    })
                    .unwrap_or(false)
            };

            PlaybackInfo {
                position: get_f64("position"),
                duration: get_f64("duration"),
                rate: get_f64("rate") as f32,
                playing: get_f64("rate") != 0.0,
                ready_to_play: get_bool("readyToPlay"),
                playback_buffer_empty: get_bool("playbackBufferEmpty"),
                loaded_time_ranges: Vec::new(), // TODO: Parse arrays
                seekable_time_ranges: Vec::new(),
            }
        } else {
            return Err(AirPlayError::ProtocolError {
                message: "Expected dictionary in playback info".to_string(),
            });
        };

        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playback_info_defaults() {
        let info = PlaybackInfo {
            position: 0.0,
            duration: 100.0,
            rate: 1.0,
            playing: true,
            ready_to_play: true,
            playback_buffer_empty: false,
            loaded_time_ranges: Vec::new(),
            seekable_time_ranges: Vec::new(),
        };

        assert!(info.playing);
        assert_eq!(info.duration, 100.0);
    }
}
```

---

## Acceptance Criteria

- [ ] Can initiate URL playback
- [ ] Can scrub/seek within content
- [ ] Can pause/resume
- [ ] Playback info is parsed correctly
- [ ] Stop cleanly terminates session

---

## Notes

- URL streaming puts less load on client (device does decoding)
- Not all devices support all URL formats
- May need to handle DRM-protected content differently
- Consider adding media type detection
