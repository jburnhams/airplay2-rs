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

#[test]
fn test_parse_set_rate_command_int() {
    let mut dict = HashMap::new();
    dict.insert(
        "type".to_string(),
        PlistValue::String("setPlaybackRate".to_string()),
    );
    dict.insert("rate".to_string(), PlistValue::Integer(0));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    match cmd {
        PlaybackCommand::SetRate { rate } => assert!((rate - 0.0).abs() < f32::EPSILON),
        _ => panic!("Expected SetRate"),
    }
}

#[test]
fn test_parse_set_rate_command_real() {
    let mut dict = HashMap::new();
    dict.insert(
        "type".to_string(),
        PlistValue::String("setPlaybackRate".to_string()),
    );
    dict.insert("rate".to_string(), PlistValue::Real(0.5));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    match cmd {
        PlaybackCommand::SetRate { rate } => assert!((rate - 0.5).abs() < f32::EPSILON),
        _ => panic!("Expected SetRate"),
    }
}

#[test]
fn test_parse_set_rate_command_default() {
    let mut dict = HashMap::new();
    dict.insert(
        "type".to_string(),
        PlistValue::String("setPlaybackRate".to_string()),
    );
    // Missing rate defaults to 1.0
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    match cmd {
        PlaybackCommand::SetRate { rate } => assert!((rate - 1.0).abs() < f32::EPSILON),
        _ => panic!("Expected SetRate"),
    }
}

#[test]
fn test_parse_seek_command_negative() {
    let mut dict = HashMap::new();
    dict.insert(
        "type".to_string(),
        PlistValue::String("seekToPosition".to_string()),
    );
    dict.insert("position".to_string(), PlistValue::Integer(-30000));
    let plist = PlistValue::Dictionary(dict);

    // Should return None when position is negative due to try_from
    let cmd = PlaybackCommand::from_plist(&plist);
    assert!(cmd.is_none());
}
