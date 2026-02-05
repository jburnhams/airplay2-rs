//! SET_PARAMETER request routing

use crate::protocol::rtsp::RtspRequest;
use super::volume_handler::{parse_volume_parameter, VolumeUpdate};
use super::metadata_handler::{parse_dmap_metadata, TrackMetadata};
use super::artwork_handler::{parse_artwork, Artwork};
use super::progress_handler::{parse_progress, PlaybackProgress};

/// Result of processing SET_PARAMETER
#[derive(Debug)]
pub enum ParameterUpdate {
    /// Volume update
    Volume(VolumeUpdate),
    /// Track metadata update
    Metadata(TrackMetadata),
    /// Album artwork update
    Artwork(Artwork),
    /// Playback progress update
    Progress(PlaybackProgress),
    /// Unknown parameter type
    Unknown(String),
}

/// Process SET_PARAMETER request
pub fn process_set_parameter(request: &RtspRequest) -> Vec<ParameterUpdate> {
    let mut updates = Vec::new();

    let content_type = request.headers.get("Content-Type")
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
