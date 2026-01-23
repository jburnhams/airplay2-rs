use super::mock_server::{MockServer, MockServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn test_config() -> MockServerConfig {
    MockServerConfig {
        rtsp_port: 0, // Use ephemeral port
        audio_port: 0,
        control_port: 0,
        timing_port: 0,
        ..MockServerConfig::default()
    }
}

#[tokio::test]
async fn test_mock_server_starts() {
    let mut server = MockServer::new(test_config());
    let addr = server.start().await.unwrap();

    assert!(addr.port() > 0);

    server.stop().await;
}

#[tokio::test]
async fn test_mock_server_accepts_connection() {
    let mut server = MockServer::new(test_config());
    let addr = server.start().await.unwrap();

    // Connect to server
    let stream = TcpStream::connect(addr).await;
    assert!(stream.is_ok());

    server.stop().await;
}

#[tokio::test]
async fn test_options() {
    let mut server = MockServer::new(test_config());
    let addr = server.start().await.unwrap();

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = "OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n";
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("RTSP/1.0 200 OK"));
    assert!(response.contains("Public:"));
    assert!(response.contains("SETUP"));
    assert!(response.contains("RECORD"));

    server.stop().await;
}

#[tokio::test]
async fn test_setup() {
    let mut server = MockServer::new(test_config());
    let addr = server.start().await.unwrap();

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = "SETUP rtsp://localhost/stream RTSP/1.0\r\nCSeq: 1\r\nTransport: RTP/AVP/UDP;unicast;mode=record\r\n\r\n";
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("RTSP/1.0 200 OK"));
    assert!(response.contains("Session:"));
    assert!(response.contains("Transport:"));
    assert!(response.contains("server_port="));

    server.stop().await;
}

#[tokio::test]
async fn test_record_pause() {
    let mut server = MockServer::new(test_config());
    let addr = server.start().await.unwrap();

    // SETUP first to get session (though mock doesn't strictly enforce sequence for simple tests, it's good practice)
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // RECORD
    let request = "RECORD rtsp://localhost/stream RTSP/1.0\r\nCSeq: 1\r\nSession: 123456\r\n\r\n";
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);
    assert!(response.contains("RTSP/1.0 200 OK"));

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    assert!(server.is_streaming().await);

    // PAUSE
    let request = "PAUSE rtsp://localhost/stream RTSP/1.0\r\nCSeq: 2\r\nSession: 123456\r\n\r\n";
    stream.write_all(request.as_bytes()).await.unwrap();

    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);
    assert!(response.contains("RTSP/1.0 200 OK"));

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    assert!(!server.is_streaming().await);

    server.stop().await;
}

#[tokio::test]
async fn test_set_parameter_volume() {
    let mut server = MockServer::new(test_config());
    let addr = server.start().await.unwrap();

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let volume_cmd = "volume: -10.5";
    let request = format!(
        "SET_PARAMETER rtsp://localhost/stream RTSP/1.0\r\nCSeq: 1\r\nContent-Length: {}\r\n\r\n{}",
        volume_cmd.len(),
        volume_cmd
    );

    stream.write_all(request.as_bytes()).await.unwrap();

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);
    assert!(response.contains("RTSP/1.0 200 OK"));

    let vol = server.volume().await;
    assert!((vol - -10.5).abs() < 0.001);

    server.stop().await;
}
