//! `AirPlay` 2 Receiver Components
//!
//! This module contains `AirPlay` 2 specific receiver functionality.
//! It builds on shared infrastructure and reuses protocol primitives
//! from the client implementation.

/// Command endpoint handler
pub mod command_handler;
/// Configuration types
pub mod config;
/// Encrypted channel handling
pub mod encrypted_channel;
/// Feedback endpoint handler
pub mod feedback_handler;
/// Info endpoint handler
pub mod info_endpoint;
/// Multi-room coordination
pub mod multi_room;
/// Pairing server implementation
pub mod pairing_server;
/// Password authentication handler
pub mod password_auth;
/// PTP clock synchronization
pub mod ptp_clock;
/// Main receiver implementation
pub mod receiver;
/// RTP decryption
pub mod rtp_decryptor;
/// Session state machine
pub mod session_state;
/// Setup handler
pub mod setup_handler;

// Re-exports
pub use config::Ap2Config;
pub use info_endpoint::InfoEndpoint;
pub use pairing_server::PairingServer;
pub use receiver::AirPlay2Receiver;
pub use session_state::Ap2SessionState;

#[cfg(test)]
mod tests;
