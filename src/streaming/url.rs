//! URL-based streaming for `AirPlay`

use crate::connection::ConnectionManager;
use crate::error::AirPlayError;
use crate::plist_dict;
use crate::protocol::plist::PlistValue;
use crate::protocol::rtsp::Method;

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
    /// Start time in seconds
    pub start: f64,
    /// Duration in seconds
    pub duration: f64,
}

impl UrlStreamer {
    /// Create a new URL streamer
    #[must_use]
    pub fn new(connection: Arc<ConnectionManager>) -> Self {
        Self {
            connection,
            current_url: None,
            playback_info: None,
        }
    }

    /// Start playing a URL
    ///
    /// # Errors
    ///
    /// Returns error if playback initiation fails
    pub async fn play(&mut self, url: &str) -> Result<(), AirPlayError> {
        // Build the play request body as plist
        let body = plist_dict![
            "Content-Location" => url,
            "Start-Position" => 0.0,
        ];

        // Send PLAY command
        self.send_command(Method::Play, Some(body)).await?;

        self.current_url = Some(url.to_string());
        Ok(())
    }

    /// Start playing at a specific position
    ///
    /// # Errors
    ///
    /// Returns error if playback initiation fails
    pub async fn play_at(&mut self, url: &str, position: f64) -> Result<(), AirPlayError> {
        let body = plist_dict![
            "Content-Location" => url,
            "Start-Position" => position,
        ];

        self.send_command(Method::Play, Some(body)).await?;

        self.current_url = Some(url.to_string());
        Ok(())
    }

    /// Get current playback info
    ///
    /// # Errors
    ///
    /// Returns error if fetching info fails
    pub async fn get_playback_info(&mut self) -> Result<PlaybackInfo, AirPlayError> {
        let response = self.send_command(Method::GetParameter, None).await?;

        // Parse response body as plist
        let info = Self::parse_playback_info(&response)?;
        self.playback_info = Some(info.clone());

        Ok(info)
    }

    /// Scrub (seek) to position
    ///
    /// # Errors
    ///
    /// Returns error if seeking fails
    pub async fn scrub(&self, position: f64) -> Result<(), AirPlayError> {
        let body = plist_dict![
            "position" => position,
        ];

        self.send_command(Method::SetParameter, Some(body)).await?;
        Ok(())
    }

    /// Set playback rate (0.0 = pause, 1.0 = play)
    ///
    /// # Errors
    ///
    /// Returns error if rate setting fails
    pub async fn set_rate(&self, rate: f32) -> Result<(), AirPlayError> {
        let body = plist_dict![
            "rate" => f64::from(rate),
        ];

        self.send_command(Method::SetParameter, Some(body)).await?;
        Ok(())
    }

    /// Pause playback
    ///
    /// # Errors
    ///
    /// Returns error if pause fails
    pub async fn pause(&self) -> Result<(), AirPlayError> {
        self.set_rate(0.0).await
    }

    /// Resume playback
    ///
    /// # Errors
    ///
    /// Returns error if resume fails
    pub async fn resume(&self) -> Result<(), AirPlayError> {
        self.set_rate(1.0).await
    }

    /// Stop playback
    ///
    /// # Errors
    ///
    /// Returns error if stop fails
    pub async fn stop(&mut self) -> Result<(), AirPlayError> {
        self.send_command(Method::Teardown, None).await?;
        self.current_url = None;
        self.playback_info = None;
        Ok(())
    }

    /// Get current position
    #[must_use]
    pub fn position(&self) -> Option<Duration> {
        self.playback_info
            .as_ref()
            .map(|info| Duration::from_secs_f64(info.position))
    }

    /// Get total duration
    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        self.playback_info
            .as_ref()
            .map(|info| Duration::from_secs_f64(info.duration))
    }

    /// Check if playing
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playback_info.as_ref().is_some_and(|info| info.playing)
    }

    /// Send RTSP command
    async fn send_command(
        &self,
        method: Method,
        body: Option<PlistValue>,
    ) -> Result<Vec<u8>, AirPlayError> {
        let body_bytes = if let Some(body) = body {
            Some(
                crate::protocol::plist::encode(&body).map_err(|e| AirPlayError::CodecError {
                    message: format!("Failed to encode plist: {e}"),
                })?,
            )
        } else {
            None
        };

        self.connection.send_command(method, body_bytes, None).await
    }

    /// Parse playback info from response
    pub(crate) fn parse_playback_info(data: &[u8]) -> Result<PlaybackInfo, AirPlayError> {
        // Parse plist response
        let plist = crate::protocol::plist::decode(data).map_err(|e| AirPlayError::CodecError {
            message: format!("Failed to parse playback info: {e}"),
        })?;

        // Extract fields from plist
        if let PlistValue::Dictionary(dict) = plist {
            let get_f64 =
                |key: &str| -> f64 { dict.get(key).and_then(PlistValue::as_f64).unwrap_or(0.0) };

            let get_bool = |key: &str| -> bool {
                dict.get(key).and_then(PlistValue::as_bool).unwrap_or(false)
            };

            let get_time_ranges = |key: &str| -> Vec<TimeRange> {
                dict.get(key)
                    .and_then(PlistValue::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                v.as_dict().map(|d| TimeRange {
                                    start: d
                                        .get("start")
                                        .and_then(PlistValue::as_f64)
                                        .unwrap_or(0.0),
                                    duration: d
                                        .get("duration")
                                        .and_then(PlistValue::as_f64)
                                        .unwrap_or(0.0),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };

            Ok(PlaybackInfo {
                position: get_f64("position"),
                duration: get_f64("duration"),
                // Rate is typically 0.0 or 1.0, so precision loss is acceptable
                #[allow(clippy::cast_possible_truncation)]
                rate: get_f64("rate") as f32,
                playing: get_f64("rate") != 0.0,
                ready_to_play: get_bool("readyToPlay"),
                playback_buffer_empty: get_bool("playbackBufferEmpty"),
                loaded_time_ranges: get_time_ranges("loadedTimeRanges"),
                seekable_time_ranges: get_time_ranges("seekableTimeRanges"),
            })
        } else {
            Err(AirPlayError::CodecError {
                message: "Expected dictionary in playback info".to_string(),
            })
        }
    }
}
