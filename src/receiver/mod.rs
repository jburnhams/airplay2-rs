//! Receiver implementation for AirPlay
//!
//! This module contains the server-side logic for accepting AirPlay sessions.

pub mod announce_handler;
pub mod audio_pipeline;
pub mod rtsp_handler;
pub mod session;
pub mod session_manager;

pub mod control_receiver;
pub mod playback_timing;
pub mod receiver_manager;
pub mod rtp_receiver;
pub mod sequence_tracker;
pub mod timing;

#[cfg(test)]
mod tests;
