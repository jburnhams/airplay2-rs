pub mod mock_server;
#[cfg(test)]
/// Unit tests for the mock server.
pub mod tests;

use crate::types::{AirPlayDevice, DeviceCapabilities};
use std::collections::HashMap;
use std::net::IpAddr;

/// Helper to create an `AirPlayDevice` for testing.
///
/// This bypasses discovery and directly populates fields, including private ones.
#[must_use]
pub fn create_test_device(id: &str, name: &str, address: IpAddr, port: u16) -> AirPlayDevice {
    AirPlayDevice {
        id: id.to_string(),
        name: name.to_string(),
        model: Some("TestModel".to_string()),
        address,
        port,
        capabilities: DeviceCapabilities::default(),
        txt_records: HashMap::new(),
    }
}
