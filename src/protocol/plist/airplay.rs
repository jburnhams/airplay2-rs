use super::{DictBuilder, PlistValue};
use crate::types::{PlaybackInfo, TrackInfo};

/// Convert `TrackInfo` to plist dictionary for `AirPlay` protocol
pub fn track_info_to_plist(track: &TrackInfo) -> PlistValue {
    DictBuilder::new()
        .insert("Content-Location", track.url.as_str())
        .insert("title", track.title.as_str())
        .insert("artist", track.artist.as_str())
        .insert_opt("album", track.album.as_deref())
        .insert_opt("artworkURL", track.artwork_url.as_deref())
        .insert_opt("duration", track.duration_secs)
        .insert_opt("trackNumber", track.track_number.map(i64::from))
        .insert_opt("discNumber", track.disc_number.map(i64::from))
        .build()
}

/// Parse playback state from device response plist
pub fn parse_playback_info(plist: &PlistValue) -> Option<PlaybackInfo> {
    let _dict = plist.as_dict()?;

    // Parse position, rate, duration, etc.
    // Implementation details based on protocol analysis
    // For now we leave this as todo as we haven't defined the mapping yet
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_info_to_plist() {
        let track = TrackInfo::new("http://url", "Title", "Artist")
            .with_album("Album")
            .with_duration(123.0);

        let plist = track_info_to_plist(&track);
        let dict = plist.as_dict().unwrap();

        assert_eq!(
            dict.get("title").and_then(super::super::PlistValue::as_str),
            Some("Title")
        );
        assert_eq!(
            dict.get("duration")
                .and_then(super::super::PlistValue::as_f64),
            Some(123.0)
        );
    }
}
