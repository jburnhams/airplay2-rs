//! Connection management

mod manager;
mod state;

pub use manager::ConnectionManager;
pub use state::{ConnectionEvent, ConnectionState, ConnectionStats, DisconnectReason};

#[cfg(test)]
mod tests;
