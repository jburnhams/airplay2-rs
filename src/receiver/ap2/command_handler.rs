//! /command Endpoint Handler

use std::collections::HashMap;

use super::body_handler::{encode_bplist_body, parse_bplist_body, PlistExt};
use super::request_handler::{Ap2Event, Ap2HandleResult, Ap2RequestContext};
use super::response_builder::Ap2ResponseBuilder;
use crate::protocol::plist::PlistValue;
use crate::protocol::rtsp::{RtspRequest, StatusCode};

/// Playback command types
#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    /// Play command
    Play,
    /// Pause command
    Pause,
    /// Stop command
    Stop,
    /// Skip to next item
    SkipNext,
    /// Skip to previous item
    SkipPrevious,
    /// Seek to a specific position
    Seek {
        /// Position in milliseconds
        position_ms: u64,
    },
    /// Set the playback rate
    SetRate {
        /// Playback rate (e.g. 1.0 for play, 0.0 for pause)
        rate: f32,
    },
    /// Unknown command
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
                Some(Self::Seek {
                    position_ms: position,
                })
            }
            "setPlaybackRate" => {
                // Rate is typically 0.0 (pause) or 1.0 (play)
                #[allow(clippy::cast_precision_loss)]
                let rate = plist.get_int("rate").map_or(1.0, |i| i as f32);
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
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: Some(format!("Failed to parse command: {e}")),
            };
        }
    };

    // Extract command
    let command = PlaybackCommand::from_plist(&plist);

    tracing::debug!("Received command: {:?}", command);

    // Build response
    let response_plist = PlistValue::Dictionary({
        let mut d = HashMap::new();
        d.insert("status".to_string(), PlistValue::Integer(0)); // Success
        d
    });

    let body = match encode_bplist_body(&response_plist) {
        Ok(b) => b,
        Err(e) => {
            return Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::INTERNAL_ERROR)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: Some(format!("Failed to encode response: {e}")),
            };
        }
    };

    let event = command.as_ref().map(|cmd| Ap2Event::CommandReceived {
        command: format!("{cmd:?}"),
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
        response: Ap2ResponseBuilder::ok().cseq(cseq).encode(),
        new_state: None,
        event: None,
        error: None,
    }
}
