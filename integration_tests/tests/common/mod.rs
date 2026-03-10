pub mod audio_verify;
pub mod diagnostics;
pub mod ports;
pub mod python_receiver;
pub mod subprocess;

use std::sync::Once;

use tracing_subscriber::{EnvFilter, fmt};

#[allow(dead_code)]
static INIT: Once = Once::new();

#[allow(dead_code)]
pub fn init_logging() {
    INIT.call_once(|| {
        let filter = EnvFilter::from_default_env().add_directive("airplay2=debug".parse().unwrap());
        fmt().with_env_filter(filter).with_test_writer().init();
    });
}
