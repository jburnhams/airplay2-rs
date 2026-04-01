use std::time::Duration;

use airplay2::testing::create_test_device;
use airplay2::{AirPlayClient, AirPlayConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn test_raop_handshake_compliance() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mut device = create_test_device("raop-test-id", "RAOP Device", addr.ip(), addr.port());
    device.raop_port = Some(addr.port());

    let client = AirPlayClient::new(AirPlayConfig::default());

    let connect_handle = tokio::spawn(async move { client.connect(&device).await });

    let (mut stream, _) = listener.accept().await.unwrap();

    let mut buffer = [0u8; 4096];
    let mut read_buf = Vec::new();
    let mut sent_pair_setup = false;

    loop {
        let n = tokio::time::timeout(Duration::from_millis(500), stream.read(&mut buffer)).await;
        let n = match n {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => n,
            Ok(Err(_)) => break,
            Err(_) => break, // Timeout
        };
        read_buf.extend_from_slice(&buffer[..n]);

        while let Some(pos) = read_buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let request_bytes = read_buf[..pos].to_vec();
            let request = String::from_utf8_lossy(&request_bytes);
            let first_line = request.lines().next().unwrap_or("").to_string();

            let content_len = request
                .lines()
                .find_map(|l| {
                    if l.to_lowercase().starts_with("content-length:") {
                        l.split(':')
                            .nth(1)
                            .unwrap_or("")
                            .trim()
                            .parse::<usize>()
                            .ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            if read_buf.len() < pos + 4 + content_len {
                break; // Need more data for body
            }

            let _body = &read_buf[pos + 4..pos + 4 + content_len];
            let request_str = request.to_string();

            // Extract CSeq if present
            let cseq = request
                .lines()
                .find_map(|l| {
                    if l.to_lowercase().starts_with("cseq:") {
                        l.split(':')
                            .nth(1)
                            .unwrap_or("")
                            .trim()
                            .parse::<usize>()
                            .ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            println!("Received request: {} (CSeq: {})", first_line, cseq);

            let proto = if first_line.contains("HTTP/1.1") {
                "HTTP/1.1"
            } else {
                "RTSP/1.0"
            };

            if request_str.starts_with("OPTIONS") {
                let response = format!(
                    "{} 200 OK\r\nCSeq: {}\r\nPublic: ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, \
                     TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER, POST, \
                     GET\r\nApple-Jack-Status: connected; type=analog\r\n\r\n",
                    proto, cseq
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            } else if request_str.starts_with("GET /info") {
                let response = format!(
                    "{} 200 OK\r\nCSeq: {}\r\nContent-Type: \
                     application/x-apple-binary-plist\r\nContent-Length: 0\r\n\r\n",
                    proto, cseq
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            } else if request_str.starts_with("POST /auth-setup") {
                let body_res = [0u8; 32];
                let response = format!(
                    "{} 200 OK\r\nCSeq: {}\r\nContent-Type: \
                     application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                    proto,
                    cseq,
                    body_res.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.write_all(&body_res).await.unwrap();
                tokio::time::sleep(Duration::from_millis(50)).await;
            } else if request_str.starts_with("POST /pair-setup") {
                let response = format!(
                    "{} 200 OK\r\nCSeq: {}\r\nContent-Length: 0\r\n\r\n",
                    proto, cseq
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                tokio::time::sleep(Duration::from_millis(50)).await;
                sent_pair_setup = true;
            } else if request_str.starts_with("POST /pair-verify") {
                let response = format!(
                    "{} 200 OK\r\nCSeq: {}\r\nContent-Length: 0\r\n\r\n",
                    proto, cseq
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                tokio::time::sleep(Duration::from_millis(50)).await;
            } else {
                let response = format!(
                    "{} 200 OK\r\nCSeq: {}\r\nContent-Length: 0\r\n\r\n",
                    proto, cseq
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }

            // Remove processed request and body from buffer
            read_buf.drain(..pos + 4 + content_len);
        }

        if sent_pair_setup {
            // "add a small sleep (e.g., 50ms) before dropping the socket"
            tokio::time::sleep(Duration::from_millis(50)).await;
            break; // Let's not drop the stream prematurely, maybe client needs to reconnect or fail correctly.
            // Let the stream drop by falling out of scope and breaking the loop.
        }
    }

    // Explicitly drop stream
    drop(stream);

    // Some client fallback mechanisms do retries or expect failures, let's give it up to 5s.
    let result = tokio::time::timeout(Duration::from_secs(5), connect_handle).await;

    match result {
        Ok(Ok(Ok(_))) => panic!("Client connected successfully without pairing"),
        Ok(Ok(Err(e))) => {
            println!("Client failed as expected: {}", e);
        }
        Ok(Err(e)) => std::panic::resume_unwind(e.into_panic()),
        Err(_) => panic!("Timeout waiting for client to fail"),
    }
}
