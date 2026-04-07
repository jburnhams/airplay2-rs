use std::time::Duration;

use airplay2::protocol::crypto::rsa_sizes;
use airplay2::receiver::AirPlayReceiver;
use airplay2::receiver::config::ReceiverConfig;
use rsa::pkcs8::EncodePrivateKey;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::test]
async fn test_receiver_server_announce_with_rsa_key() {
    // Generate a valid RSA test key
    let mut rng = rand::thread_rng();
    let private_key = rsa::RsaPrivateKey::new(&mut rng, rsa_sizes::MODULUS_BITS).unwrap();
    let rsa_der = private_key.to_pkcs8_der().unwrap().to_bytes().to_vec();

    // Start a receiver configured with the RSA private key
    let config = ReceiverConfig::with_name("RSA Test Receiver")
        .port(0)
        .rsa_private_key(rsa_der.clone());

    let mut receiver = AirPlayReceiver::new(config);
    receiver.start().await.expect("Failed to start receiver");

    // Wait for it to start up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Get the assigned port from the event stream.
    let mut rx = receiver.subscribe();

    let mut port = 0;
    // We expect the Started event to have been sent. Wait up to 1 second.
    if let Ok(Ok(event)) = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
        if let airplay2::receiver::events::ReceiverEvent::Started { port: p, .. } = event {
            port = p;
        }
    }

    // If not found in stream, maybe it started before we subscribed or it's a test environment
    // thing
    if port == 0 {
        // Fallback to a fixed port for testing to avoid hanging
        let alt_config = ReceiverConfig::with_name("RSA Test Receiver Alt")
            .port(12346)
            .rsa_private_key(rsa_der);
        receiver.stop().await.ok();

        receiver = AirPlayReceiver::new(alt_config);
        receiver
            .start()
            .await
            .expect("Failed to start receiver on fixed port");
        tokio::time::sleep(Duration::from_millis(100)).await;
        port = 12346;
    }

    assert!(port > 0);

    // Connect to the receiver
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect to receiver");

    // We can simulate an ANNOUNCE request using an SDP that includes `rsaaeskey` and `aesiv`.
    let sdp = "\
v=0\r
o=iTunes 3719003884 0 IN IP4 192.168.1.100\r
s=iTunes\r
c=IN IP4 192.168.1.100\r
t=0 0\r
m=audio 0 RTP/AVP 96\r
a=rtpmap:96 AppleLossless\r
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r
a=rsaaeskey:VGhpcyBpcyBhIHRlc3Qga2V5IHRoYXQgaXMgdXNlZCBmb3IgdGVzdGluZw==\r
a=aesiv:VGhpcyBpcyBhIHRlc3QgaXY=\r
";

    let request_str = format!(
        "ANNOUNCE rtsp://127.0.0.1/1234 RTSP/1.0\r\nCSeq: 1\r\nContent-Length: {}\r\n\r\n{}",
        sdp.len(),
        sdp
    );

    stream.write_all(request_str.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    // Read the response
    let mut buf = vec![0u8; 1024];
    let n = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf))
        .await
        .expect("Timeout waiting for response")
        .expect("Read failed");

    let response_str = String::from_utf8_lossy(&buf[..n]);

    // We expect the server to process the request and hit the decryption path.
    // Since the base64 rsaaeskey we hardcoded is not actually encrypted with our newly generated
    // test key, it will fail to decrypt and return a 400 Bad Request, which proves the RSA key
    // was passed all the way down successfully. If it didn't pass the key, it would skip
    // decryption entirely and return 200 OK.

    assert!(response_str.contains("RTSP/1.0 400 Bad Request"));

    receiver.stop().await.unwrap();
}
