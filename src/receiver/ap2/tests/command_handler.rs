use std::collections::HashMap;

use crate::protocol::plist::PlistValue;
use crate::receiver::ap2::command_handler::PlaybackCommand;

#[test]
fn test_parse_play_command() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("play".to_string()));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::Play));
}

#[test]
fn test_parse_seek_command() {
    let mut dict = HashMap::new();
    dict.insert(
        "type".to_string(),
        PlistValue::String("seekToPosition".to_string()),
    );
    dict.insert("position".to_string(), PlistValue::Integer(30000));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::Seek { position_ms: 30000 }));
}
