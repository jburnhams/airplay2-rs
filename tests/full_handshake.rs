use airplay2::receiver::{AirPlayReceiver, ReceiverConfig, ReceiverEvent};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::test]
async fn test_full_handshake() {
    // 1. Start Receiver
    let config = ReceiverConfig::with_name("Handshake Test").port(0);
    let mut receiver = AirPlayReceiver::new(config);
    let mut events = receiver.subscribe();

    receiver.start().await.unwrap();

    // Get port
    let port = loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        match event {
            ReceiverEvent::Started { port: p, .. } => break p,
            _ => continue,
        }
    };

    // 2. Connect Client
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();

    // 3. Send OPTIONS
    let options_req = "OPTIONS * RTSP/1.0\r\n\
                       CSeq: 1\r\n\
                       User-Agent: AirPlay/320.20\r\n\
                       \r\n";
    stream.write_all(options_req.as_bytes()).await.unwrap();

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("RTSP/1.0 200 OK"));
    assert!(response.contains("Public: ANNOUNCE, SETUP, RECORD"));
    assert!(response.contains("CSeq: 1"));

    // 4. Send ANNOUNCE
    let sdp = "v=0\r\n\
               o=- 123456 0 IN IP4 127.0.0.1\r\n\
               s=AirTunes\r\n\
               c=IN IP4 127.0.0.1\r\n\
               t=0 0\r\n\
               m=audio 0 RTP/AVP 96\r\n\
               a=rtpmap:96 AppleLossless\r\n\
               a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n";

    let announce_req = format!(
        "ANNOUNCE rtsp://127.0.0.1/1234 RTSP/1.0\r\n\
         CSeq: 2\r\n\
         Content-Type: application/sdp\r\n\
         Content-Length: {}\r\n\
         \r\n\
         {}",
        sdp.len(),
        sdp
    );
    stream.write_all(announce_req.as_bytes()).await.unwrap();

    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("RTSP/1.0 200 OK"));
    assert!(response.contains("CSeq: 2"));

    // 5. Send SETUP
    let setup_req = "SETUP rtsp://127.0.0.1/1234/stream RTSP/1.0\r\n\
                     CSeq: 3\r\n\
                     Transport: RTP/AVP/UDP;unicast;mode=record;timing_port=6000;control_port=6001\r\n\
                     \r\n";
    stream.write_all(setup_req.as_bytes()).await.unwrap();

    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("RTSP/1.0 200 OK"));
    assert!(response.contains("Session:"));
    assert!(response.contains("Transport:"));
    assert!(response.contains("server_port="));

    // Extract Session ID
    let session_line = response
        .lines()
        .find(|l| l.starts_with("Session:"))
        .unwrap();
    let session_id = session_line.split(':').nth(1).unwrap().trim().to_string();

    // 6. Send RECORD
    let record_req = format!(
        "RECORD rtsp://127.0.0.1/1234/stream RTSP/1.0\r\n\
         CSeq: 4\r\n\
         Session: {}\r\n\
         RTP-Info: seq=1;rtptime=12345\r\n\
         \r\n",
        session_id
    );
    stream.write_all(record_req.as_bytes()).await.unwrap();

    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("RTSP/1.0 200 OK"));
    assert!(response.contains("Audio-Latency:"));

    // 7. Verify PlaybackStarted event
    loop {
        let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap();
        match event {
            ReceiverEvent::PlaybackStarted => break,
            _ => continue,
        }
    }

    // 8. Send TEARDOWN
    let teardown_req = format!(
        "TEARDOWN rtsp://127.0.0.1/1234/stream RTSP/1.0\r\n\
         CSeq: 5\r\n\
         Session: {}\r\n\
         \r\n",
        session_id
    );
    stream.write_all(teardown_req.as_bytes()).await.unwrap();

    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("RTSP/1.0 200 OK"));

    // 9. Stop Receiver
    receiver.stop().await.unwrap();
}
