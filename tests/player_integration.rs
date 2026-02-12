use airplay2::PlayerBuilder;
use airplay2::testing::{
    create_test_device,
    mock_server::{MockServer, MockServerConfig},
};
use std::time::Duration;

// Note: This test is currently ignored because the mock server doesn't fully implement
// the SRP pairing protocol. The client always attempts authentication, but the mock server
// only provides dummy responses. A full mock SRP implementation is needed.
#[tokio::test]
#[ignore]
async fn test_player_against_mock() {
    // Allocate some ports
    let p1 = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let p2 = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let p3 = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    // 1. Start Mock Server
    let config = MockServerConfig {
        rtsp_port: 0, // Ephemeral port
        audio_port: p1,
        control_port: p2,
        timing_port: p3,
        device_name: "Mock Device".to_string(),
        ..Default::default()
    };
    let mut server = MockServer::new(config);
    let addr = server.start().await.expect("Failed to start mock server");

    // 2. Create Test Device pointing to Mock Server
    let device = create_test_device("mock-id-123", "Mock Device", addr.ip(), addr.port());

    // 3. Connect Player
    let player = PlayerBuilder::new().auto_reconnect(false).build();

    player.connect(&device).await.expect("Failed to connect");
    assert!(player.is_connected().await);

    // 4. Test Playback
    let url = "http://example.com/stream";
    player
        .play_track(url, "Title", "Artist")
        .await
        .expect("Failed to play");

    // Give it a moment for the async task to hit the server
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify server state (MockServer sets streaming=true on RECORD)
    assert!(server.is_streaming().await);

    // 5. Test Volume
    player.set_volume(0.5).await.expect("Failed to set volume");

    // 6. Stop
    player.stop().await.expect("Failed to stop");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!server.is_streaming().await);

    // 7. Disconnect
    player.disconnect().await.expect("Failed to disconnect");
    assert!(!player.is_connected().await);
}
