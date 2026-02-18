use airplay2::AirPlayClient;
use airplay2::state::ClientEvent;
use airplay2::testing::mock_server::{MockServer, MockServerConfig};
use airplay2::types::AirPlayDevice;
use std::time::Duration;
use tokio::time::timeout;

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

#[tokio::test]
async fn test_client_integration_flow() {
    init_tracing();
    // 1. Start Mock Server
    // Use port 0 to let OS assign a free port
    let config = MockServerConfig {
        rtsp_port: 0,
        audio_port: 0,   // Mock server binds UDP ports too?
        control_port: 0, // Mock server doesn't seem to bind UDP ports in start(), just RTSP TCP
        timing_port: 0,
        ..Default::default()
    };
    // Note: MockServer currently only binds TCP RTSP port in start().
    // UDP ports are just used in Transport header response.
    // For this integration test, we don't need actual UDP traffic unless the client expects it immediately.

    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start mock server");

    // 2. Create Client
    let client = AirPlayClient::default_client();
    let mut events = client.subscribe_events();

    // 3. Create Device info pointing to mock server
    let device = AirPlayDevice {
        id: "mock_device_id".to_string(),
        name: "Mock Device".to_string(),
        model: Some("MockModel".to_string()),
        addresses: vec![addr.ip()],
        port: addr.port(),
        capabilities: airplay2::types::DeviceCapabilities {
            airplay2: true,
            supports_audio: true,
            supports_buffered_audio: true,
            ..Default::default()
        },
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    // 4. Connect
    println!("Connecting to mock server at {}", addr);
    timeout(Duration::from_secs(5), client.connect(&device))
        .await
        .expect("Connection timed out")
        .expect("Connection failed");

    assert!(client.is_connected().await);

    // Verify Connected event
    // The event might have been emitted during connect(), so we might have missed it if we subscribed late?
    // But we subscribed before connect().
    // Let's drain events to find Connected.
    let mut connected_event_found = false;
    while let Ok(event) = timeout(Duration::from_secs(2), events.recv()).await {
        if let ClientEvent::Connected { device: d } = event.unwrap() {
            assert_eq!(d.id, "mock_device_id");
            connected_event_found = true;
            break;
        }
    }
    assert!(connected_event_found, "Did not receive Connected event");

    // Check state
    let state = client.state().await;
    assert_eq!(state.device.unwrap().id, "mock_device_id");

    // 5. Playback Controls
    println!("Testing playback controls...");

    // Play
    client.play().await.expect("Play failed");
    // Verify server state
    // Give a tiny bit of time for network roundtrip (localhost is fast but async context switches)
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(server.is_streaming().await);
    assert!(client.playback_state().await.is_playing);

    // Set Volume
    client.set_volume(0.5).await.expect("Set volume failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    let vol = server.volume().await;
    // Client sends volume in dB. 0.5 linear -> 20*log10(0.5) = -6.02 dB
    let expected_db = 20.0 * 0.5f32.log10();
    assert!(
        (vol - expected_db).abs() < 0.1,
        "Server received volume {} dB, expected {} dB",
        vol,
        expected_db
    );
    assert!((client.volume().await - 0.5).abs() < f32::EPSILON);

    // Pause
    client.pause().await.expect("Pause failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!server.is_streaming().await);
    assert!(!client.playback_state().await.is_playing);

    // 6. Disconnect
    println!("Disconnecting...");
    client.disconnect().await.expect("Disconnect failed");
    assert!(!client.is_connected().await);

    // Verify Disconnected event
    // Ignore other events like VolumeChanged
    let mut disconnected = false;
    while let Ok(event) = timeout(Duration::from_secs(1), events.recv()).await {
        if let ClientEvent::Disconnected { reason, .. } = event.unwrap() {
            assert!(reason.contains("UserRequested"));
            disconnected = true;
            break;
        }
    }
    assert!(disconnected, "Did not receive Disconnected event");

    server.stop().await;
}

#[tokio::test]
async fn test_client_reconnect_logic() {
    // This test verifies that we can disconnect and reconnect
    let config = MockServerConfig {
        rtsp_port: 0,
        ..Default::default()
    };
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start mock server");

    let client = AirPlayClient::default_client();
    let device = AirPlayDevice {
        id: "mock_device_reconnect".to_string(),
        name: "Mock Device Reconnect".to_string(),
        model: Some("MockModel".to_string()),
        addresses: vec![addr.ip()],
        port: addr.port(),
        capabilities: Default::default(),
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    // First connection
    client
        .connect(&device)
        .await
        .expect("First connection failed");
    assert!(client.is_connected().await);
    client.disconnect().await.expect("First disconnect failed");
    assert!(!client.is_connected().await);

    // Second connection
    client
        .connect(&device)
        .await
        .expect("Second connection failed");
    assert!(client.is_connected().await);
    client.disconnect().await.expect("Second disconnect failed");
    assert!(!client.is_connected().await);

    server.stop().await;
}
