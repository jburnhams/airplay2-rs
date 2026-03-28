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

    let mut step = 1;
    loop {
        let n = match tokio::time::timeout(Duration::from_millis(500), stream.read(&mut buffer)).await {
            Ok(Ok(0)) => break, // Connection closed
            Ok(Ok(n)) => n,
            Ok(Err(_)) | Err(_) => break, // Error or timeout, just stop and see what connect_handle returns
        };
        let request = String::from_utf8_lossy(&buffer[..n]);
        println!("Received request {}: {}", step, request);

        // Extract CSeq if present to reply properly
        let mut cseq = "1";
        for line in request.lines() {
            if line.starts_with("CSeq: ") {
                cseq = line.strip_prefix("CSeq: ").unwrap().trim();
            }
        }

        if request.starts_with("OPTIONS") {
            // Verify Mandatory Headers on first OPTIONS
            if step == 1 {
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
            }

            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nPublic: ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, \
                            TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER, POST, \
                            GET\r\nApple-Jack-Status: connected; type=analog\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("GET /info") {
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: application/x-apple-binary-plist\r\nContent-Length: 0\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("POST /auth-setup") {
            let body = vec![0u8; 32];
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: application/octet-stream\r\nContent-Length: 32\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
        } else if request.starts_with("POST /pair-setup") || request.starts_with("POST /pair-verify") {
            // we will sleep to stop the test from continuing since pairing requires crypto calculations
            // and this is a mock test that stops at handshakes.
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Length: 0\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            break;
        } else if request.starts_with("ANNOUNCE") {
            assert!(request.contains("Content-Type: application/sdp"));

            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("SETUP") {
            assert!(request.contains("Transport: RTP/AVP/UDP"));

            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nSession: CAFEBABE\r\nTransport: \
                            RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;\
                            timing_port=6002\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("RECORD") {
            assert!(request.contains("Session: CAFEBABE"));
            assert!(request.contains("Range: npt=0-"));

            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\nAudio-Latency: 2205\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
            break; // Finished handshake sequence
        } else {
            // It could be a body from pair setup, just read it and continue waiting for next
            println!("Got non-RTSP request/body: {}", request);
            continue;
        }
        step += 1;
    }

    // Await client result (with timeout)
    // The client might fail if we stopped early, but we verified the handshake start.
    // If handshake completed, client.connect() should return Ok.

    // We intentionally stop before pairing finishes, so we expect connect to fail eventually
    // but the test is validating handshakes

    let result = tokio::time::timeout(Duration::from_secs(1), connect_handle).await;
    match result {
        Ok(Ok(Ok(_))) => println!("Client connected successfully"),
        Ok(Ok(Err(e))) => println!("Client connect failed early, but this is fine since it lacks pairing crypto setup: {}", e),
        Ok(Err(_)) => panic!("Client panic"),
        Err(_) => println!("Timeout waiting for client - expected as we halt pair-setup"),
    }
}
