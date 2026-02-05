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
