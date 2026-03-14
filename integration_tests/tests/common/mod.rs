pub mod diagnostics;
pub mod ports;
pub mod python_receiver;
pub mod shairport_sync;
#[cfg(test)]
pub mod shairport_sync_tests;
pub mod subprocess;

use std::sync::Once;

#[allow(dead_code, reason = "Used in some test modules but not all")]
static INIT: Once = Once::new();

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub fn init_logging() {
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("info")
            .with_test_writer()
            .try_init();
    });
}
