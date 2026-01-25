//! mDNS device discovery for `AirPlay` devices

mod browser;
pub mod parser;
/// RAOP discovery logic
pub mod raop;
#[cfg(test)]
mod raop_tests;
#[cfg(test)]
mod tests;

pub use browser::{DeviceBrowser, DeviceFilter, DiscoveryEvent, DiscoveryOptions};
pub use parser::parse_txt_records;

use crate::error::AirPlayError;
use crate::types::{AirPlayConfig, AirPlayDevice};
use futures::Stream;
use std::time::Duration;

/// Service type for `AirPlay` discovery
pub const AIRPLAY_SERVICE_TYPE: &str = "_airplay._tcp.local.";

pub use raop::RAOP_SERVICE_TYPE;

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
/// # async fn example() -> Result<(), airplay2::AirPlayError> {
/// let mut devices = discover().await?;
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
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if the mDNS daemon cannot be initialized.
#[allow(clippy::unused_async)]
pub async fn discover() -> Result<impl Stream<Item = DiscoveryEvent>, AirPlayError> {
    discover_with_config(AirPlayConfig::default()).await
}

/// Discover devices with custom configuration
///
/// # Errors
///
/// Returns an error if the mDNS daemon cannot be initialized.
#[allow(clippy::unused_async)]
pub async fn discover_with_config(
    config: AirPlayConfig,
) -> Result<impl Stream<Item = DiscoveryEvent>, AirPlayError> {
    let browser = DeviceBrowser::new(&config);
    browser.browse()
}

/// Discover devices with custom options
///
/// # Errors
///
/// Returns an error if the mDNS daemon cannot be initialized.
#[allow(clippy::unused_async)]
pub async fn discover_with_options(
    options: DiscoveryOptions,
) -> Result<impl Stream<Item = DiscoveryEvent>, AirPlayError> {
    let browser = DeviceBrowser::with_options(options);
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
///     println!("{}: {}", device.name, device.address());
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

    let browser = DeviceBrowser::new(&config);
    let stream = browser.browse()?;

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

/// Scan for devices with custom options
///
/// # Errors
///
/// Returns an error if the mDNS daemon cannot be initialized.
pub async fn scan_with_options(
    options: DiscoveryOptions,
) -> Result<Vec<AirPlayDevice>, AirPlayError> {
    use futures::StreamExt;
    use std::collections::HashMap;

    let timeout = options.timeout;
    let browser = DeviceBrowser::with_options(options);
    let stream = browser.browse()?;

    let mut devices: HashMap<String, AirPlayDevice> = HashMap::new();
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
