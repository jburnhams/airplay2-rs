//! `AirPlay` 2 Receiver Components
//!
//! This module contains `AirPlay` 2 specific receiver functionality.
//! It builds on shared infrastructure and reuses protocol primitives
//! from the client implementation.

/// Configuration types
pub mod config;
/// Main receiver implementation
pub mod receiver;
/// Pairing server implementation
pub mod pairing_server;
/// Password authentication handler
pub mod password_auth;
/// Info endpoint handler
pub mod info_endpoint;
/// Setup handler
pub mod setup_handler;
/// Encrypted channel handling
pub mod encrypted_channel;
/// RTP decryption
pub mod rtp_decryptor;
/// PTP clock synchronization
pub mod ptp_clock;
/// Command endpoint handler
pub mod command_handler;
/// Feedback endpoint handler
pub mod feedback_handler;
/// Multi-room coordination
pub mod multi_room;
/// Session state machine
pub mod session_state;

// Re-exports
pub use config::Ap2Config;
pub use receiver::AirPlay2Receiver;
pub use pairing_server::PairingServer;
pub use info_endpoint::InfoEndpoint;
pub use session_state::Ap2SessionState;

#[cfg(test)]
mod tests;
