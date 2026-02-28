use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use tokio::time::sleep;

use crate::client::{ClientConfig, PreferredProtocol, UnifiedAirPlayClient};
use crate::testing::mock_raop_server::{MockRaopConfig, MockRaopServer};
use crate::types::{AirPlayDevice, DeviceCapabilities};

async fn create_device_with_server() -> (AirPlayDevice, MockRaopServer) {
    let config = MockRaopConfig {
        rtsp_port: 0,  // Dynamic
        audio_port: 0, // Dynamic
        control_port: 0,
        timing_port: 0,
        ..Default::default()
    };
    let mut server = MockRaopServer::new(config);
    server.start().await.expect("failed to start mock server");

    let device = AirPlayDevice {
        id: "test-streaming".to_string(),
        name: "Test Streaming Device".to_string(),
        model: None,
        addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        port: 7000,
        capabilities: DeviceCapabilities::default(),
        raop_port: Some(server.config.rtsp_port),
        raop_capabilities: None,
        txt_records: std::collections::HashMap::new(),
    };

    (device, server)
}

#[tokio::test]
async fn test_raop_audio_streaming() {
    let (device, server) = create_device_with_server().await;

    // Configure client to force RAOP
    let config = ClientConfig {
        preferred_protocol: PreferredProtocol::ForceRaop,
        ..Default::default()
    };
    let mut client = UnifiedAirPlayClient::with_config(config);

    // Connect
    client.connect(device).await.expect("Failed to connect");
    assert!(client.is_connected());

    // Stream some dummy audio data
    // RaopStreamer expects raw PCM (or ALAC) frames.
    // For this test, we just send bytes to verify they reach the server.
    // The streamer will wrap them in RTP + encryption.
    let dummy_audio = vec![0xAB; 352 * 4]; // One packet worth of 16-bit stereo PCM

    // Send a few packets
    for _ in 0..5 {
        client
            .stream_audio(&dummy_audio)
            .await
            .expect("Failed to stream audio");
        // Yield to allow async tasks to run
        sleep(Duration::from_millis(20)).await;
    }

    // Give some time for UDP packets to arrive
    sleep(Duration::from_millis(200)).await;

    // Check server received packets
    {
        let state = server.state.lock().unwrap();
        assert!(
            !state.audio_packets.is_empty(),
            "Server should have received audio packets"
        );
        println!("Server received {} packets", state.audio_packets.len());

        // Verify packet content (encrypted so we can't easily check payload without decrypting,
        // but we can check if it looks like an RTP packet)
        // RTP Header is 12 bytes.
        let packet = &state.audio_packets[0];
        assert!(packet.len() > 12, "Packet too short for RTP header");
        assert_eq!(packet[0] & 0xC0, 0x80, "Invalid RTP version"); // Version 2
    }

    client.disconnect().await.expect("Failed to disconnect");
}
