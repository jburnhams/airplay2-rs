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

pub mod artwork_handler;
pub mod metadata_handler;
pub mod progress_handler;
pub mod set_parameter_handler;
pub mod volume_handler;

#[cfg(test)]
mod tests;
