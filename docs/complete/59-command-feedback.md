# Section 59: /command & /feedback Endpoints

## Dependencies
- **Section 48**: RTSP/HTTP Server Extensions
- **Section 03**: Binary Plist Codec

## Overview

The `/command` and `/feedback` endpoints handle playback control and status reporting. These are POST endpoints that use binary plist bodies.

## Objectives

- Handle playback commands (play, pause, stop, seek)
- Process feedback requests
- Support progress reporting
- Emit appropriate events

---

## Tasks

### 59.1 Command Handler

**File:** `src/receiver/ap2/command_handler.rs`

```rust
//! /command Endpoint Handler

use super::request_handler::{Ap2HandleResult, Ap2Event, Ap2RequestContext};
use super::response_builder::Ap2ResponseBuilder;
use super::body_handler::{parse_bplist_body, PlistExt, encode_bplist_body};
use crate::protocol::plist::PlistValue;
use crate::protocol::rtsp::{RtspRequest, StatusCode};
use std::collections::HashMap;

/// Playback command types
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Stop,
    SkipNext,
    SkipPrevious,
    Seek { position_ms: u64 },
    SetRate { rate: f32 },
    Unknown(String),
}

impl PlaybackCommand {
    /// Parse from plist
    pub fn from_plist(plist: &PlistValue) -> Option<Self> {
        let cmd_type = plist.get_string("type")?;

        match cmd_type {
            "play" => Some(Self::Play),
            "pause" => Some(Self::Pause),
            "stop" => Some(Self::Stop),
            "nextItem" | "skipNext" => Some(Self::SkipNext),
            "previousItem" | "skipPrevious" => Some(Self::SkipPrevious),
            "seekToPosition" => {
                let position = plist.get_int("position")? as u64;
                Some(Self::Seek { position_ms: position })
            }
            "setPlaybackRate" => {
                // Rate is typically 0.0 (pause) or 1.0 (play)
                let rate = plist.get_int("rate").map(|i| i as f32).unwrap_or(1.0);
                Some(Self::SetRate { rate })
            }
            other => Some(Self::Unknown(other.to_string())),
        }
    }
}

/// Handle POST /command
pub fn handle_command(
    request: &RtspRequest,
    cseq: u32,
    _context: &Ap2RequestContext,
) -> Ap2HandleResult {
    // Parse command body
    let plist = match parse_bplist_body(&request.body) {
        Ok(p) => p,
        Err(e) => {
            return Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::BAD_REQUEST)
                    .cseq(cseq).encode(),
                new_state: None,
                event: None,
                error: Some(format!("Failed to parse command: {}", e)),
            };
        }
    };

    // Extract command
    let command = PlaybackCommand::from_plist(&plist);

    log::debug!("Received command: {:?}", command);

    // Build response
    let response_plist = PlistValue::Dict({
        let mut d = HashMap::new();
        d.insert("status".to_string(), PlistValue::Integer(0));  // Success
        d
    });

    let body = match encode_bplist_body(&response_plist) {
        Ok(b) => b,
        Err(e) => {
            return Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::INTERNAL_ERROR)
                    .cseq(cseq).encode(),
                new_state: None,
                event: None,
                error: Some(format!("Failed to encode response: {}", e)),
            };
        }
    };

    let event = command.as_ref().map(|cmd| {
        Ap2Event::CommandReceived {
            command: format!("{:?}", cmd),
        }
    });

    Ap2HandleResult {
        response: Ap2ResponseBuilder::ok()
            .cseq(cseq)
            .header("Content-Type", "application/x-apple-binary-plist")
            .binary_body(body)
            .encode(),
        new_state: None,
        event,
        error: None,
    }
}

/// Handle POST /feedback
pub fn handle_feedback(
    request: &RtspRequest,
    cseq: u32,
    _context: &Ap2RequestContext,
) -> Ap2HandleResult {
    // Feedback is typically empty or contains timing info
    let _plist = parse_bplist_body(&request.body);

    // Just acknowledge
    Ap2HandleResult {
        response: Ap2ResponseBuilder::ok()
            .cseq(cseq)
            .encode(),
        new_state: None,
        event: None,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_play_command() {
        let mut dict = HashMap::new();
        dict.insert("type".to_string(), PlistValue::String("play".to_string()));
        let plist = PlistValue::Dict(dict);

        let cmd = PlaybackCommand::from_plist(&plist).unwrap();
        assert!(matches!(cmd, PlaybackCommand::Play));
    }

    #[test]
    fn test_parse_seek_command() {
        let mut dict = HashMap::new();
        dict.insert("type".to_string(), PlistValue::String("seekToPosition".to_string()));
        dict.insert("position".to_string(), PlistValue::Integer(30000));
        let plist = PlistValue::Dict(dict);

        let cmd = PlaybackCommand::from_plist(&plist).unwrap();
        assert!(matches!(cmd, PlaybackCommand::Seek { position_ms: 30000 }));
    }
}
```

---

## Acceptance Criteria

 - [x] Command parsing from binary plist
 - [x] All command types handled
 - [x] Feedback endpoint acknowledged
 - [x] Events emitted for commands
 - [x] All unit tests pass

---

## References

- [AirPlay 2 Protocol Analysis](https://emanuelecozzi.net/docs/airplay2/)
