pub mod mock_server;
#[cfg(test)]
/// Unit tests for the mock server.
pub mod tests;

use std::collections::HashMap;
use std::net::IpAddr;

use crate::types::{AirPlayDevice, DeviceCapabilities};

/// Helper to create an `AirPlayDevice` for testing.
///
/// This bypasses discovery and directly populates fields, including private ones.
#[must_use]
pub fn create_test_device(id: &str, name: &str, address: IpAddr, port: u16) -> AirPlayDevice {
    AirPlayDevice {
        id: id.to_string(),
        name: name.to_string(),
        model: Some("TestModel".to_string()),
        addresses: vec![address],
        port,
        capabilities: DeviceCapabilities::default(),
        raop_port: None,
        raop_capabilities: None,
        txt_records: HashMap::new(),
    }
}
