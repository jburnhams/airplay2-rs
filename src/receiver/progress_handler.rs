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
