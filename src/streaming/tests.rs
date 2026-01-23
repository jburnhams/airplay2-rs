use super::*;
use crate::audio::AudioFormat;

#[test]
fn test_slice_source() {
    let data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
    let mut source = SliceSource::new(data.clone(), AudioFormat::CD_QUALITY);

    let mut buffer = vec![0u8; 4];
    let n = source.read(&mut buffer).unwrap();
    assert_eq!(n, 4);
    assert_eq!(buffer, vec![1, 2, 3, 4]);

    let n = source.read(&mut buffer).unwrap();
    assert_eq!(n, 4);
    assert_eq!(buffer, vec![5, 6, 7, 8]);

    let n = source.read(&mut buffer).unwrap();
    assert_eq!(n, 0); // EOF
}

#[test]
fn test_silence_source() {
    let mut source = SilenceSource::new(AudioFormat::CD_QUALITY);

    let mut buffer = vec![255u8; 100];
    let n = source.read(&mut buffer).unwrap();

    assert_eq!(n, 100);
    assert!(buffer.iter().all(|&b| b == 0));
}

#[test]
fn test_callback_source() {
    let format = AudioFormat::CD_QUALITY;
    let mut counter = 0;
    let mut source = CallbackSource::new(format, move |buf: &mut [u8]| {
        counter += 1;
        buf.fill(counter);
        Ok(buf.len())
    });

    let mut buffer = vec![0u8; 4];
    source.read(&mut buffer).unwrap();
    assert_eq!(buffer, vec![1, 1, 1, 1]);

    source.read(&mut buffer).unwrap();
    assert_eq!(buffer, vec![2, 2, 2, 2]);
}

#[tokio::test]
async fn test_pcm_streamer_creation() {
    use crate::connection::ConnectionManager;
    use crate::types::AirPlayConfig;
    use std::sync::Arc;

    let config = AirPlayConfig::default();
    let connection = Arc::new(ConnectionManager::new(config));
    let format = AudioFormat::CD_QUALITY;

    let streamer = PcmStreamer::new(connection, format);
    assert_eq!(streamer.state().await, StreamerState::Idle);
}

use crate::error::AirPlayError;
use crate::streaming::RtpSender;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tokio::time::Duration;

#[derive(Default)]
struct MockRtpSender {
    packets: Arc<Mutex<Vec<Vec<u8>>>>,
}

#[async_trait]
impl RtpSender for MockRtpSender {
    async fn send_rtp_audio(&self, packet: &[u8]) -> Result<(), AirPlayError> {
        self.packets.lock().unwrap().push(packet.to_vec());
        Ok(())
    }
}

#[tokio::test]
async fn test_streaming_loop() {
    let sender = Arc::new(MockRtpSender::default());
    let packets = sender.packets.clone();

    let format = AudioFormat::CD_QUALITY;
    let streamer = PcmStreamer::new(sender, format);

    // Create source
    let data = vec![1u8; 20000]; // Should produce many packets
    let source = SliceSource::new(data, format);

    // Start streaming in background
    let streamer_arc = Arc::new(streamer);
    let s = streamer_arc.clone();

    let handle = tokio::spawn(async move { s.stream(source).await });

    // Allow some time for streaming
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Check state (might be Streaming or finished if fast, but with interval it should be streaming)
    assert_eq!(streamer_arc.state().await, StreamerState::Streaming);

    // Pause
    streamer_arc.pause().await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(streamer_arc.state().await, StreamerState::Paused);

    // Resume
    streamer_arc.resume().await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(streamer_arc.state().await, StreamerState::Streaming);

    // Stop
    streamer_arc.stop().await.unwrap();
    let result = handle.await.unwrap();
    assert!(result.is_ok());
    assert_eq!(streamer_arc.state().await, StreamerState::Idle);

    // Check packets
    let sent = packets.lock().unwrap();
    assert!(!sent.is_empty());
}

#[tokio::test]
async fn test_url_streamer_creation() {
    use crate::connection::ConnectionManager;
    use crate::streaming::url::UrlStreamer;
    use crate::types::AirPlayConfig;
    use std::sync::Arc;

    let config = AirPlayConfig::default();
    let connection = Arc::new(ConnectionManager::new(config));

    let streamer = UrlStreamer::new(connection);
    assert!(!streamer.is_playing());
}
