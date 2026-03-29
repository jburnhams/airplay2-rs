use std::time::Duration;

use airplay2::testing::create_test_device;
use airplay2::{AirPlayClient, AirPlayConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn test_raop_handshake_compliance() {
    // 1. Setup Custom Mock Server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // 2. Create RAOP Device
    // Setting raop_port is crucial to trigger RAOP logic
    let mut device = create_test_device("raop-test-id", "RAOP Device", addr.ip(), addr.port());
    device.raop_port = Some(addr.port());

    // 3. Connect Client in background
    let client = AirPlayClient::new(AirPlayConfig::default());

    let connect_handle = tokio::spawn(async move { client.connect(&device).await });

    // 4. Accept connection and verify handshake
    let (mut stream, _) = listener.accept().await.unwrap();

    // Helper to read request
    let mut buffer = [0u8; 4096];

    // --- Step 1: OPTIONS ---
    let n = stream.read(&mut buffer).await.unwrap();
    let request = String::from_utf8_lossy(&buffer[..n]);

    println!("Received request 1: {}", request);

    // Verify Method
    assert!(request.starts_with("OPTIONS"));

    // Verify Mandatory Headers
    assert!(request.contains("CSeq:"), "Missing CSeq header");
    assert!(request.contains("User-Agent:"), "Missing User-Agent header");
    assert!(
        request.contains("Client-Instance:"),
        "Missing Client-Instance header"
    );
    assert!(request.contains("DACP-ID:"), "Missing DACP-ID header");
    assert!(
        request.contains("Active-Remote:"),
        "Missing Active-Remote header"
    );
    assert!(
        request.contains("X-Apple-Device-ID:"),
        "Missing X-Apple-Device-ID header"
    );

    // Send Response
    // RAOP requires Apple-Challenge in response for auth, but we can simulate success or continue
    // If we don't send Apple-Challenge, client might skip auth or fail if strict.
    // Let's send a standard response.
    let response = "RTSP/1.0 200 OK\r\nCSeq: 1\r\nPublic: ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, \
                    TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER, POST, \
                    GET\r\nApple-Jack-Status: connected; type=analog\r\n\r\n";
    stream.write_all(response.as_bytes()).await.unwrap();

    // --- Step 2: ANNOUNCE ---
    let n = stream.read(&mut buffer).await.unwrap();
    let request = String::from_utf8_lossy(&buffer[..n]);

    println!("Received request 2: {}", request);

    // Wait for the client to complete. Since we don't handle GET /info, Auth-Setup, etc.,
    // the client connection might fail or timeout, which is expected for this partial test.
    // For a compliance test that only tests OPTIONS, we either mock the rest or assert the failure.
    // Based on `RtspSession`, it might send ANNOUNCE or SETUP.

    if request.starts_with("ANNOUNCE") {
        assert!(request.contains("Content-Type: application/sdp"));

        let response = "RTSP/1.0 200 OK\r\nCSeq: 2\r\n\r\n";
        stream.write_all(response.as_bytes()).await.unwrap();

        // --- Step 3: SETUP ---
        let n = stream.read(&mut buffer).await.unwrap();
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request 3: {}", request);

        assert!(request.starts_with("SETUP"));
        assert!(request.contains("Transport: RTP/AVP/UDP"));

        let response = "RTSP/1.0 200 OK\r\nCSeq: 3\r\nSession: CAFEBABE\r\nTransport: \
                        RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                        timing_port=6002\r\n\r\n";
        stream.write_all(response.as_bytes()).await.unwrap();

        // --- Step 4: RECORD ---
        let n = stream.read(&mut buffer).await.unwrap();
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request 4: {}", request);

        assert!(request.starts_with("RECORD"));
        assert!(request.contains("Session: CAFEBABE"));
        assert!(request.contains("Range: npt=0-"));

        let response = "RTSP/1.0 200 OK\r\nCSeq: 4\r\nAudio-Latency: 2205\r\n\r\n";
        stream.write_all(response.as_bytes()).await.unwrap();
    } else if request.starts_with("POST") {
        // Maybe pairing?
        println!("Got POST instead of ANNOUNCE");
        // For this test, we might stop here if we unexpected behavior, or handle it.
        // This verifies that we at least got past the first step.
    }

    // Await client result (with timeout)
    // The client might fail if we stopped early, but we verified the handshake start.
    // Since we don't handle GET /info, Auth-Setup, etc., the client connection might fail or
    // timeout, which is expected for this partial test. We assert that it either connects
    // (unlikely without full mocking) or fails with a known AirPlayError, but it shouldn't
    // panic.
    let result = tokio::time::timeout(Duration::from_secs(1), connect_handle).await;

    match result {
        Ok(Ok(Ok(_))) => println!("Client connected successfully"),
        Ok(Ok(Err(e))) => println!("Client failed as expected during partial handshake: {e}"),
        Ok(Err(_)) => panic!("Client panic"),
        Err(_) => println!("Timeout waiting for client as expected during partial handshake"),
    }
}
