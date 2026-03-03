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
fn test_parse_missing_type() {
    let mut dict = HashMap::new();
    dict.insert("position".to_string(), PlistValue::Integer(30000));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist);
    assert!(cmd.is_none());
}

#[test]
fn test_parse_pause_command() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("pause".to_string()));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::Pause));
}

#[test]
fn test_parse_stop_command() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("stop".to_string()));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::Stop));
}

#[test]
fn test_parse_skip_next_command() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("skipNext".to_string()));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::SkipNext));

    let mut dict2 = HashMap::new();
    dict2.insert("type".to_string(), PlistValue::String("nextItem".to_string()));
    let plist2 = PlistValue::Dictionary(dict2);
    let cmd2 = PlaybackCommand::from_plist(&plist2).unwrap();
    assert!(matches!(cmd2, PlaybackCommand::SkipNext));
}

#[test]
fn test_parse_skip_previous_command() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("skipPrevious".to_string()));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::SkipPrevious));

    let mut dict2 = HashMap::new();
    dict2.insert("type".to_string(), PlistValue::String("previousItem".to_string()));
    let plist2 = PlistValue::Dictionary(dict2);
    let cmd2 = PlaybackCommand::from_plist(&plist2).unwrap();
    assert!(matches!(cmd2, PlaybackCommand::SkipPrevious));
}

#[test]
fn test_parse_set_rate_command() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("setPlaybackRate".to_string()));
    dict.insert("rate".to_string(), PlistValue::Integer(1));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::SetRate { rate } if (rate - 1.0).abs() < f32::EPSILON));
}

#[test]
fn test_parse_set_rate_default() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("setPlaybackRate".to_string()));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::SetRate { rate } if (rate - 1.0).abs() < f32::EPSILON));
}

#[test]
fn test_parse_unknown_command() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("customCommand".to_string()));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    if let PlaybackCommand::Unknown(cmd_str) = cmd {
        assert_eq!(cmd_str, "customCommand");
    } else {
        panic!("Expected Unknown command");
    }
}

#[test]
fn test_parse_seek_missing_position() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("seekToPosition".to_string()));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist);
    assert!(cmd.is_none());
}
