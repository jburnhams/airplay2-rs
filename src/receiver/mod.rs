//! Receiver implementation for AirPlay
//!
//! This module contains the server-side logic for accepting AirPlay sessions.

pub mod announce_handler;
pub mod rtsp_handler;
pub mod session;
pub mod session_manager;

#[cfg(test)]
mod announce_handler_tests;
#[cfg(test)]
mod rtsp_handler_tests;
#[cfg(test)]
mod session_tests;
