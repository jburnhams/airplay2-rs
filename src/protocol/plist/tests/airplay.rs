use crate::protocol::plist::{PlistValue, airplay::track_info_to_plist};
use crate::types::TrackInfo;

#[test]
fn test_track_info_to_plist() {
    let track = TrackInfo::new("http://url", "Title", "Artist")
        .with_album("Album")
        .with_duration(123.0);

    let plist = track_info_to_plist(&track);
    let dict = plist.as_dict().unwrap();

    assert_eq!(
        dict.get("title").and_then(PlistValue::as_str),
        Some("Title")
    );
    assert_eq!(
        dict.get("duration").and_then(PlistValue::as_f64),
        Some(123.0)
    );
}
