//! Common test utilities and fixtures
#![allow(dead_code)]

use std::sync::Once;

use tracing_subscriber::{EnvFilter, fmt};

pub mod python_receiver;

static INIT: Once = Once::new();

/// Initialize test logging (call once per test module)
pub fn init_logging() {
    INIT.call_once(|| {
        let filter = EnvFilter::from_default_env().add_directive("airplay2=debug".parse().unwrap());

        fmt().with_env_filter(filter).with_test_writer().init();
    });
}

/// Create a test configuration with short timeouts
pub fn test_config() -> airplay2::AirPlayConfig {
    airplay2::AirPlayConfig {
        discovery_timeout: std::time::Duration::from_millis(100),
        connection_timeout: std::time::Duration::from_millis(500),
        state_poll_interval: std::time::Duration::from_millis(50),
        debug_protocol: true,
        ..Default::default()
    }
}
