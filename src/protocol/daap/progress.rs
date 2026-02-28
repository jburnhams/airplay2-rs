//! Playback progress for RAOP

/// Playback progress information
#[derive(Debug, Clone, Copy)]
pub struct DmapProgress {
    /// RTP timestamp of track start
    pub start: u32,
    /// RTP timestamp of current position
    pub current: u32,
    /// RTP timestamp of track end
    pub end: u32,
}

impl DmapProgress {
    /// Create new progress
    #[must_use]
    pub fn new(start: u32, current: u32, end: u32) -> Self {
        Self {
            start,
            current,
            end,
        }
    }

    /// Create progress for track at given position
    ///
    /// # Arguments
    /// * `base_timestamp` - RTP timestamp at track start
    /// * `position_samples` - Current position in samples
    /// * `duration_samples` - Total duration in samples
    #[must_use]
    pub fn from_samples(base_timestamp: u32, position_samples: u32, duration_samples: u32) -> Self {
        Self {
            start: base_timestamp,
            current: base_timestamp.wrapping_add(position_samples),
            end: base_timestamp.wrapping_add(duration_samples),
        }
    }

    /// Encode as text/parameters body
    #[must_use]
    pub fn encode(&self) -> String {
        format!("progress: {}/{}/{}\r\n", self.start, self.current, self.end)
    }

    /// Get current position in seconds (at 44.1kHz)
    #[must_use]
    pub fn position_secs(&self) -> f64 {
        let samples = self.current.wrapping_sub(self.start);
        f64::from(samples) / 44100.0
    }

    /// Get duration in seconds (at 44.1kHz)
    #[must_use]
    pub fn duration_secs(&self) -> f64 {
        let samples = self.end.wrapping_sub(self.start);
        f64::from(samples) / 44100.0
    }

    /// Get progress as percentage (0.0 - 1.0)
    #[must_use]
    pub fn percentage(&self) -> f64 {
        let total = f64::from(self.end.wrapping_sub(self.start));
        if total == 0.0 {
            return 0.0;
        }
        let current = f64::from(self.current.wrapping_sub(self.start));
        (current / total).clamp(0.0, 1.0)
    }

    /// Parse from text/parameters body
    #[must_use]
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
