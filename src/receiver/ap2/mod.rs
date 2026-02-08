//! `AirPlay` 2 Receiver Components
//!
//! This module contains `AirPlay` 2 specific receiver functionality.
//! It builds on shared infrastructure and reuses protocol primitives
//! from the client implementation.

pub mod config;
pub mod session_state;
// pub mod receiver;
// pub mod pairing_server;
// pub mod password_auth;
// pub mod info_endpoint;
// pub mod setup_handler;
// pub mod encrypted_channel;
// pub mod rtp_decryptor;
// pub mod ptp_clock;
// pub mod command_handler;
// pub mod feedback_handler;
// pub mod multi_room;

#[cfg(test)]
mod tests;

// Re-exports
pub use config::Ap2Config;
pub use session_state::Ap2SessionState;
// pub use receiver::AirPlay2Receiver;
// pub use pairing_server::PairingServer;
// pub use info_endpoint::InfoEndpoint;
