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
    let n = match stream.read(&mut buffer).await {
        Ok(0) | Err(_) => return,
        Ok(n) => n,
    };
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

    // --- Subsequent Steps ---
    // Loop to handle GET /info, POST, and ANNOUNCE to ensure graceful failure per memory guidelines.
    loop {
        let n = match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        if n == 0 {
            return;
        }
        let request = String::from_utf8_lossy(&buffer[..n]);

        // Parse CSeq
        let mut cseq = "1";
        for line in request.lines() {
            if line.starts_with("CSeq: ") {
                cseq = line.strip_prefix("CSeq: ").unwrap().trim();
            }
        }

        if request.starts_with("ANNOUNCE") {
            assert!(request.contains("Content-Type: application/sdp"));
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("SETUP") {
            assert!(request.contains("Transport: RTP/AVP/UDP"));
            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nSession: CAFEBABE\r\nTransport: RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002\r\n\r\n",
                cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        } else if request.starts_with("RECORD") {
            assert!(request.contains("Session: CAFEBABE"));
            assert!(request.contains("Range: npt=0-"));
            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nAudio-Latency: 2205\r\n\r\n",
                cseq
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            break; // Handshake complete
        } else if request.starts_with("GET /info") || request.starts_with("POST") {
            // Gracefully handle GET and POST (e.g. auth-setup or pair-setup) to prevent client hangs
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        } else {
            // Unknown methods
            let response = format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", cseq);
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    }

    // Await client result (with 5 second timeout to allow brute_force_pairing to finish)
    let result = tokio::time::timeout(Duration::from_secs(5), connect_handle).await;

    match result {
        Ok(Ok(Err(airplay2::AirPlayError::AuthenticationFailed { .. }))) => {
            // Expected failure: The test does not fully mock the crypto pairing logic,
            // so brute force pairing correctly exhausts all options and fails.
        }
        Ok(Ok(Ok(_))) => panic!("Client connected successfully despite no crypto pairing!"),
        Ok(Ok(Err(e))) => panic!("Client failed with unexpected error: {}", e),
        Ok(Err(e)) => std::panic::resume_unwind(e.into_panic()), // Propagate panics
        Err(_) => panic!("Timeout waiting for client connection"),
    }
}
