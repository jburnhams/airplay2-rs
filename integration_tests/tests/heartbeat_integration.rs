use std::time::Duration;

use airplay2::discovery::{DiscoveryEvent, discover};
use futures_util::stream::StreamExt;

mod common;
use common::python_receiver::PythonReceiver;

#[tokio::test]
async fn test_device_presence_heartbeat() -> Result<(), Box<dyn std::error::Error>> {
    common::init_logging();
    tracing::info!("Starting device presence heartbeat test");

    // Start discovery
    let mut devices = discover()?;

    // Start receiver with accelerated 100ms heartbeats
    let receiver = PythonReceiver::start_with_args(&["--heartbeat-ms", "100"]).await?;
    let device_config = receiver.device_config();

    tracing::info!("Waiting for device to be discovered...");

    // We expect an Added event, and then occasionally Updated events as the device re-announces
    let mut initial_last_seen = None;
    let mut discovered = false;
    let mut heartbeats_received = 0;

    let timeout_duration = Duration::from_secs(5); // Shorter timeout for faster tests
    let start_time = std::time::Instant::now();

    while start_time.elapsed() < timeout_duration {
        match tokio::time::timeout(Duration::from_secs(2), devices.next()).await {
            Ok(Some(DiscoveryEvent::Added(device))) => {
                if device.id == device_config.id {
                    tracing::info!("Device discovered: {}", device.name);
                    discovered = true;
                    initial_last_seen = device.last_seen;
                }
            }
            Ok(Some(DiscoveryEvent::Updated(device))) => {
                if device.id == device_config.id {
                    tracing::info!(
                        "Device updated (heartbeat): last_seen = {:?}",
                        device.last_seen
                    );
                    if let (Some(initial), Some(current)) = (initial_last_seen, device.last_seen) {
                        if current > initial {
                            heartbeats_received += 1;
                            // Update our initial last seen to the current one
                            initial_last_seen = Some(current);
                        }
                    }
                }
            }
            Ok(Some(_)) => {} // Ignore other events
            Ok(None) => break,
            Err(_) => {
                // Timeout, no new events in 2s
            }
        }

        // The Python receiver registers its mDNS service which triggers Added.
        // It might re-announce soon or we can force it or wait.
        // Actually, zeroconf might send a couple of updates shortly after startup.
        if heartbeats_received >= 1 {
            break;
        }
    }

    assert!(discovered, "Device was not discovered");

    // As seen in check_mdns, mdns-sd usually emits multiple ServiceResolved events
    // shortly after startup due to multiple PTR/TXT records resolving.
    assert!(
        heartbeats_received >= 1,
        "Did not receive any heartbeat/update for the device"
    );

    tracing::info!("✓ Heartbeat test passed");

    Ok(())
}
