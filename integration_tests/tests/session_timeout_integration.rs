#![cfg(test)]

use std::time::Duration;

use airplay2::connection::ConnectionState;
use airplay2::{AirPlayClient, AirPlayConfig};
use tokio::time::timeout;

mod common;
use common::python_receiver::PythonReceiver;

#[tokio::test]
async fn test_session_timeout_disconnect() {
    common::init_logging();

    // 1. Start a python receiver
    let receiver = PythonReceiver::start()
        .await
        .expect("Failed to start receiver");
    let device = receiver.device_config();

    // 2. Initialize an AirPlayClient with a short session timeout
    let session_timeout = Duration::from_secs(2);
    let config = AirPlayConfig::builder()
        .connection_timeout(Duration::from_secs(5))
        .session_timeout(session_timeout)
        .build();
    let client = AirPlayClient::new(config);

    // Subscribe to client events to monitor connection state
    let mut _events = client.subscribe_events();

    // 3. Connect to the receiver
    let connect_result = timeout(Duration::from_secs(10), client.connect(&device)).await;
    assert!(connect_result.is_ok(), "Connect operation timed out");
    assert!(
        connect_result.unwrap().is_ok(),
        "Failed to connect to receiver"
    );

    // Verify connection state
    assert_eq!(client.connection_state().await, ConnectionState::Connected);

    // 4. Manually override the last_activity to simulate an idle connection.
    // We fetch the ConnectionManager internals (though it's pub(crate), we can't easily access it from integration tests outside the crate).
    // Wait, integration tests are in `integration_tests/` crate, so we can't access `pub(crate)` fields like `last_activity`.
    // Instead of overriding, we'll just wait for the timeout. But wait, the client automatically sends keep-alives every 10 seconds!
    // Since our session timeout is 2 seconds, and the client sends keep-alives every 10 seconds, the keep-alive won't happen before the timeout triggers.
    // The keep-alive loop checks `last_activity` every 1 second. Since `session_timeout` is 2s, it will timeout before the first 10s keep-alive is sent.

    // 5. Wait for the session timeout to trigger (should take slightly over 2 seconds)
    // We wait 4 seconds to be safe.
    tokio::time::sleep(Duration::from_secs(4)).await;

    // 6. Assert that the client has disconnected due to session timeout
    let current_state = client.connection_state().await;
    assert_eq!(
        current_state,
        ConnectionState::Disconnected,
        "Client should have disconnected due to session timeout"
    );
}
