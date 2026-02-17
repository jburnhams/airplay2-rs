//! `AirPlay` 2 Receiver Components
//!
//! This module contains `AirPlay` 2 specific receiver functionality.
//! It builds on shared infrastructure and reuses protocol primitives
//! from the client implementation.

pub mod advertisement;
pub mod body_handler;
pub mod capabilities;
pub mod config;
pub mod encrypted_channel;
pub mod encrypted_rtsp;
pub mod features;
pub mod info_endpoint;
pub mod pairing_handlers;
pub mod pairing_server;
pub mod password_auth;
pub mod password_integration;
pub mod request_handler;
pub mod request_router;
pub mod response_builder;
pub mod rtp_decryptor;
pub mod rtp_receiver;
pub mod session_state;
pub mod setup_handler;
pub mod stream;
// pub mod receiver;
// pub mod ptp_clock;
// pub mod command_handler;
// pub mod feedback_handler;
// pub mod multi_room;

#[cfg(test)]
mod tests;

// Re-exports
pub use advertisement::{Ap2ServiceAdvertiser, Ap2TxtRecord};
pub use capabilities::DeviceCapabilities;
pub use config::Ap2Config;
pub use features::{FeatureFlag, FeatureFlags, StatusFlag, StatusFlags};
pub use info_endpoint::InfoEndpoint;
pub use pairing_server::PairingServer;
pub use password_auth::{PasswordAuthError, PasswordAuthManager};
pub use password_integration::{AuthMode, AuthenticationHandler};
pub use session_state::Ap2SessionState;
pub use setup_handler::SetupHandler;
pub use stream::StreamType;
// pub use receiver::AirPlay2Receiver;
