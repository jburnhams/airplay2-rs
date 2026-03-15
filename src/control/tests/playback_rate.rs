#[tokio::test]
async fn test_fast_forward_rewind() {
    use crate::AirPlayClient;
    use crate::testing::mock_server::{MockServer, MockServerConfig};
    use crate::types::AirPlayDevice;

    let config = MockServerConfig {
        rtsp_port: 0,
        ..Default::default()
    };
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start mock server");

    let client = AirPlayClient::default_client();
    let device = AirPlayDevice {
        id: "mock_device_rate".to_string(),
        name: "Mock Device".to_string(),
        model: Some("MockModel".to_string()),
        addresses: vec![addr.ip()],
        port: addr.port(),
        capabilities: Default::default(),
        raop_port: None,
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
        last_seen: None,
    };

    client
        .connect(&device)
        .await
        .expect("Connection failed");

    // Fast forward should set rate to 2.0
    client.fast_forward().await.expect("Fast forward failed");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(server.current_rate().await, Some(2.0));

    // Rewind should set rate to -2.0
    client.rewind().await.expect("Rewind failed");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(server.current_rate().await, Some(-2.0));

    client.disconnect().await.expect("Disconnect failed");
    server.stop().await;
}
