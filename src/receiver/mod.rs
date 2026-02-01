//! Receiver implementation for AirPlay
//!
//! This module contains the server-side logic for accepting AirPlay sessions.

pub mod announce_handler;
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
mod announce_handler_tests;
#[cfg(test)]
mod control_receiver_tests;
#[cfg(test)]
mod rtp_receiver_tests;
#[cfg(test)]
mod rtsp_handler_tests;
#[cfg(test)]
mod sequence_tracker_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tests;
