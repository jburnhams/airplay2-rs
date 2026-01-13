//! Discovery module

use crate::AirPlayDevice;
use std::time::Duration;

/// Discover `AirPlay` devices on the network.
///
/// # Errors
///
/// Returns an error if discovery fails.
#[allow(clippy::unused_async)]
pub async fn discover() -> Result<Vec<AirPlayDevice>, crate::AirPlayError> {
    Ok(vec![])
}

/// Scan for `AirPlay` devices with a timeout.
///
/// # Errors
///
/// Returns an error if the scan fails.
#[allow(clippy::unused_async)]
pub async fn scan(_timeout: Duration) -> Result<Vec<AirPlayDevice>, crate::AirPlayError> {
    Ok(vec![])
}
