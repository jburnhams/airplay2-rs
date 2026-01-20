//! mDNS device discovery for `AirPlay` devices

mod browser;
pub mod parser;
#[cfg(test)]
mod tests;

pub use browser::{DeviceBrowser, DiscoveryEvent};
pub use parser::parse_txt_records;

use crate::error::AirPlayError;
use crate::types::{AirPlayConfig, AirPlayDevice};
use futures::Stream;
use std::time::Duration;

/// Service type for `AirPlay` discovery
pub const AIRPLAY_SERVICE_TYPE: &str = "_airplay._tcp.local.";

/// Service type for `AirPlay` 2 RAOP (audio)
pub const RAOP_SERVICE_TYPE: &str = "_raop._tcp.local.";

/// Discover `AirPlay` devices continuously
///
/// Returns a stream that yields devices as they are discovered.
/// The stream continues until dropped.
///
/// # Example
///
/// ```rust,no_run
/// use airplay2::discovery::{discover, DiscoveryEvent};
/// use futures::StreamExt;
///
/// # async fn example() {
/// let mut devices = discover().await;
///
/// while let Some(event) = devices.next().await {
///     match event {
///         DiscoveryEvent::Added(device) => {
///             println!("Found: {}", device.name);
///         }
///         DiscoveryEvent::Removed(device_id) => {
///             println!("Lost: {}", device_id);
///         }
///         _ => {}
///     }
/// }
/// # }
/// ```
#[allow(clippy::unused_async)]
pub async fn discover() -> impl Stream<Item = DiscoveryEvent> {
    discover_with_config(AirPlayConfig::default()).await
}

/// Discover devices with custom configuration
#[allow(clippy::unused_async)]
pub async fn discover_with_config(config: AirPlayConfig) -> impl Stream<Item = DiscoveryEvent> {
    let browser = DeviceBrowser::new(config);
    browser.browse()
}

/// Scan for devices with timeout
///
/// Performs a one-shot scan and returns all discovered devices.
///
/// # Arguments
///
/// * `timeout` - How long to scan for devices
///
/// # Example
///
/// ```rust,no_run
/// use airplay2::discovery::scan;
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), airplay2::AirPlayError> {
/// let devices = scan(Duration::from_secs(5)).await?;
///
/// for device in devices {
///     println!("{}: {}", device.name, device.address);
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if the mDNS daemon cannot be initialized.
pub async fn scan(timeout: Duration) -> Result<Vec<AirPlayDevice>, AirPlayError> {
    scan_with_config(timeout, AirPlayConfig::default()).await
}

/// Scan for devices with custom configuration
///
/// # Errors
///
/// Returns an error if the mDNS daemon cannot be initialized.
pub async fn scan_with_config(
    timeout: Duration,
    config: AirPlayConfig,
) -> Result<Vec<AirPlayDevice>, AirPlayError> {
    use futures::StreamExt;
    use std::collections::HashMap;

    let browser = DeviceBrowser::new(config);
    let stream = browser.browse();

    let mut devices: HashMap<String, AirPlayDevice> = HashMap::new();

    // Use timeout
    let deadline = tokio::time::Instant::now() + timeout;

    tokio::pin!(stream);

    loop {
        tokio::select! {
            () = tokio::time::sleep_until(deadline) => {
                break;
            }
            event = stream.next() => {
                match event {
                    Some(DiscoveryEvent::Added(device) | DiscoveryEvent::Updated(device)) => {
                        devices.insert(device.id.clone(), device);
                    }
                    Some(DiscoveryEvent::Removed(id)) => {
                        devices.remove(&id);
                    }
                    None => break,
                }
            }
        }
    }

    Ok(devices.into_values().collect())
}
