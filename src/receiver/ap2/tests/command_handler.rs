use std::collections::HashMap;

use crate::protocol::plist::PlistValue;
use crate::protocol::rtsp::{Method, RtspRequest};
use crate::receiver::ap2::body_handler::{encode_bplist_body, parse_bplist_body};
use crate::receiver::ap2::command_handler::{PlaybackCommand, handle_command, handle_feedback};
use crate::receiver::ap2::request_handler::{Ap2Event, Ap2RequestContext};
use crate::receiver::ap2::session_state::Ap2SessionState;

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
fn test_handle_command_success() {
    let mut dict = HashMap::new();
    dict.insert("type".to_string(), PlistValue::String("play".to_string()));
    let plist = PlistValue::Dictionary(dict);
    let body = encode_bplist_body(&plist).unwrap();

    let mut request = RtspRequest::new(Method::Post, "/command");
    request.body = body;

    let context = Ap2RequestContext {
        state: &Ap2SessionState::Streaming,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let result = handle_command(&request, 123, &context);

    let response_str = String::from_utf8_lossy(&result.response);
    println!("Response string: {response_str}");
    assert!(response_str.starts_with("RTSP/1.0 200 OK"));
    assert!(response_str.contains("application/x-apple-binary-plist") || response_str.contains("application/octet-stream"));

    let header_end = result
        .response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let body_bytes = &result.response[header_end + 4..];

    let response_plist = parse_bplist_body(body_bytes).unwrap();
    if let PlistValue::Dictionary(dict) = response_plist {
        if let Some(PlistValue::Integer(status)) = dict.get("status") {
            assert_eq!(*status, 0);
        } else {
            panic!("Expected status integer");
        }
    } else {
        panic!("Expected dictionary");
    }

    assert!(matches!(
        result.event,
        Some(Ap2Event::CommandReceived { command }) if command == "Play"
    ));
}

#[test]
fn test_handle_command_invalid_body() {
    let mut request = RtspRequest::new(Method::Post, "/command");
    request.body = b"invalid plist data".to_vec();

    let context = Ap2RequestContext {
        state: &Ap2SessionState::Streaming,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let result = handle_command(&request, 123, &context);

    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.starts_with("RTSP/1.0 400 Bad Request"));
    assert!(result.event.is_none());
    assert!(result.error.is_some());
}

#[test]
fn test_handle_command_missing_type() {
    let mut dict = HashMap::new();
    dict.insert("position".to_string(), PlistValue::Integer(30000));
    let plist = PlistValue::Dictionary(dict);
    let body = encode_bplist_body(&plist).unwrap();

    let mut request = RtspRequest::new(Method::Post, "/command");
    request.body = body;

    let context = Ap2RequestContext {
        state: &Ap2SessionState::Streaming,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let result = handle_command(&request, 123, &context);

    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.starts_with("RTSP/1.0 400 Bad Request"));
    assert!(result.event.is_none());
    assert!(result.error.is_some());
}

#[test]
fn test_handle_feedback_empty_body() {
    let request = RtspRequest::new(Method::Post, "/feedback");

    let context = Ap2RequestContext {
        state: &Ap2SessionState::Streaming,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let result = handle_feedback(&request, 124, &context);

    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.starts_with("RTSP/1.0 200 OK"));
    assert!(result.event.is_none());
    assert!(result.error.is_none());
}

#[test]
fn test_handle_feedback_invalid_body() {
    let mut request = RtspRequest::new(Method::Post, "/feedback");
    request.body = b"invalid plist data".to_vec();

    let context = Ap2RequestContext {
        state: &Ap2SessionState::Streaming,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };

    let result = handle_feedback(&request, 125, &context);

    let response_str = String::from_utf8_lossy(&result.response);
    assert!(response_str.starts_with("RTSP/1.0 400 Bad Request"));
    assert!(result.event.is_none());
    assert!(result.error.is_some());
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
    dict.insert(
        "type".to_string(),
        PlistValue::String("skipNext".to_string()),
    );
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::SkipNext));

    let mut dict2 = HashMap::new();
    dict2.insert(
        "type".to_string(),
        PlistValue::String("nextItem".to_string()),
    );
    let plist2 = PlistValue::Dictionary(dict2);
    let cmd2 = PlaybackCommand::from_plist(&plist2).unwrap();
    assert!(matches!(cmd2, PlaybackCommand::SkipNext));
}

#[test]
fn test_parse_skip_previous_command() {
    let mut dict = HashMap::new();
    dict.insert(
        "type".to_string(),
        PlistValue::String("skipPrevious".to_string()),
    );
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::SkipPrevious));

    let mut dict2 = HashMap::new();
    dict2.insert(
        "type".to_string(),
        PlistValue::String("previousItem".to_string()),
    );
    let plist2 = PlistValue::Dictionary(dict2);
    let cmd2 = PlaybackCommand::from_plist(&plist2).unwrap();
    assert!(matches!(cmd2, PlaybackCommand::SkipPrevious));
}

#[test]
fn test_parse_set_rate_command() {
    let mut dict = HashMap::new();
    dict.insert(
        "type".to_string(),
        PlistValue::String("setPlaybackRate".to_string()),
    );
    dict.insert("rate".to_string(), PlistValue::Integer(1));
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::SetRate { rate } if (rate - 1.0).abs() < f32::EPSILON));
}

#[test]
fn test_parse_set_rate_default() {
    let mut dict = HashMap::new();
    dict.insert(
        "type".to_string(),
        PlistValue::String("setPlaybackRate".to_string()),
    );
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist).unwrap();
    assert!(matches!(cmd, PlaybackCommand::SetRate { rate } if (rate - 1.0).abs() < f32::EPSILON));
}

#[test]
fn test_parse_unknown_command() {
    let mut dict = HashMap::new();
    dict.insert(
        "type".to_string(),
        PlistValue::String("customCommand".to_string()),
    );
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
    dict.insert(
        "type".to_string(),
        PlistValue::String("seekToPosition".to_string()),
    );
    let plist = PlistValue::Dictionary(dict);

    let cmd = PlaybackCommand::from_plist(&plist);
    assert!(cmd.is_none());
}
