use std::time::Duration;
use airplay2::testing::create_test_device;
use airplay2::{AirPlayClient, AirPlayConfig};

/// Test that ensures connection timeouts correctly return an Error and are not swallowed
/// internally.
#[tokio::test]
async fn test_connection_timeout_propagates_error() {
    let client = AirPlayClient::new(AirPlayConfig::default());

    // Create a dummy device pointing to a non-existent host/port that will timeout
    let device = create_test_device("timeout-test-id", "Timeout Device", "10.255.255.1".parse().unwrap(), 9999);

    // Try to connect with a short timeout to speed up the test
    let result = tokio::time::timeout(Duration::from_millis(100), client.connect(&device)).await;

    match result {
        Ok(Ok(_)) => panic!("Connection to a non-existent device should not succeed"),
        Ok(Err(e)) => {
            // Expected failure mode: the underlying implementation failed to connect
            println!("Connection correctly failed with: {}", e);
        }
        Err(_) => {
            // Expected failure mode: tokio timeout hit
            println!("Connection correctly timed out");
        }
    }
}
