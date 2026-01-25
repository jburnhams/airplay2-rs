//! DACP (Digital Audio Control Protocol) for `AirPlay` remote control

mod commands;
mod server;
mod service;

pub use commands::{CommandResult, DacpCommand};
pub use server::{CallbackHandler, DacpHandler, DacpServer};
pub use service::{DacpService, DacpServiceConfig};

/// DACP service type for mDNS
pub const DACP_SERVICE_TYPE: &str = "_dacp._tcp.local.";

/// Default DACP port
pub const DACP_DEFAULT_PORT: u16 = 3689;

/// DACP TXT record keys
pub mod txt_keys {
    /// TXT record version
    pub const TXTVERS: &str = "txtvers";
    /// DACP version
    pub const VER: &str = "Ver";
    /// Database ID
    pub const DBID: &str = "DbId";
    /// OS information
    pub const OSSI: &str = "OSsi";
}

#[cfg(test)]
mod tests;
