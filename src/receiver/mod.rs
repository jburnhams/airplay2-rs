//! Receiver implementation for AirPlay
//!
//! This module contains the server-side logic for accepting AirPlay sessions.

pub mod announce_handler;
pub mod audio_pipeline;
pub mod config;
pub mod events;
pub mod rtsp_handler;
pub mod server;
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

pub mod ap2;

// Public exports
pub use artwork_handler::Artwork;
pub use config::ReceiverConfig;
pub use events::{EventCallback, ReceiverEvent};
pub use metadata_handler::TrackMetadata;
pub use progress_handler::PlaybackProgress;
pub use server::{AirPlayReceiver, ReceiverError, ReceiverState};
pub use session::{AudioCodec, SessionState, StreamParameters};
pub use volume_handler::VolumeUpdate;

// AirPlay 2 specific re-exports
pub use ap2::{
    AirPlay2Receiver,
    Ap2Config,
    PairingServer,
    InfoEndpoint,
};

#[cfg(test)]
mod tests;
