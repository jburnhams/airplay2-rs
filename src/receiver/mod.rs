//! Receiver implementation for AirPlay
//!
//! This module contains the server-side logic for accepting AirPlay sessions.

pub mod rtsp_handler;
pub mod session;
pub mod session_manager;

#[cfg(test)]
mod rtsp_handler_tests;
#[cfg(test)]
mod session_tests;
