//! Precision Time Protocol (PTP, IEEE 1588) implementation.
//!
//! Provides sub-millisecond clock synchronization for `AirPlay` 2
//! multi-room audio. This module is used by both the client (master)
//! and receiver (slave) sides.
//!
//! ## Standard PTP Ports
//!
//! - **319**: Event messages (Sync, `Delay_Req`) — require timestamping.
//! - **320**: General messages (`Follow_Up`, `Delay_Resp`, Announce).
//!
//! ## `AirPlay` Compact Format
//!
//! `AirPlay` 2 may use a simplified 24-byte timing packet on the
//! negotiated timing port (from SETUP). See [`AirPlayTimingPacket`].
//!
//! ## Clock Synchronization Flow
//!
//! ```text
//! Master                          Slave
//!   |--- Sync (T1) ----------------->|  (slave records T2)
//!   |--- Follow_Up (precise T1) ---->|
//!   |                                |
//!   |<---- Delay_Req (T3) ---------- |
//!   |---- Delay_Resp (T4) --------->|
//!   |                                |
//!   |  offset = ((T2-T1)+(T3-T4))/2 |
//!   |  RTT = (T4-T1) - (T3-T2)      |
//! ```

pub mod clock;
pub mod handler;
pub mod message;
pub mod timestamp;

#[cfg(test)]
mod tests;

// Re-exports for convenient access.
pub use clock::{PtpClock, PtpRole, TimingMeasurement};
pub use handler::{
    PTP_EVENT_PORT, PTP_GENERAL_PORT, PtpHandlerConfig, PtpMasterHandler, PtpSlaveHandler,
    SharedPtpClock, create_shared_clock,
};
pub use message::{
    AirPlayTimingPacket, PtpHeader, PtpMessage, PtpMessageBody, PtpMessageType, PtpParseError,
    PtpPortIdentity,
};
pub use timestamp::PtpTimestamp;
