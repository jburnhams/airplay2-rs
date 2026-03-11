use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::time::Duration;

use crate::audio::AudioFormat;
use crate::error::AirPlayError;
use crate::streaming::{PcmStreamer, RtpSender, SliceSource, StreamerState};

struct MockRtpSender {
    packets: Arc<Mutex<Vec<Vec<u8>>>>,
    control_packets: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Default for MockRtpSender {
    fn default() -> Self {
        Self {
            packets: Arc::new(Mutex::new(Vec::new())),
            control_packets: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl RtpSender for MockRtpSender {
    async fn send_rtp_audio(&self, packet: &[u8]) -> Result<(), AirPlayError> {
        self.packets.lock().unwrap().push(packet.to_vec());
        Ok(())
    }

    async fn send_time_announce(
        &self,
        _rtp_timestamp: u32,
        _sample_rate: u32,
    ) -> Result<(), AirPlayError> {
        Ok(())
    }

    async fn send_rtcp_control(&self, packet: &[u8]) -> Result<(), AirPlayError> {
        self.control_packets.lock().unwrap().push(packet.to_vec());
        Ok(())
    }

    fn subscribe_events(
        &self,
    ) -> Option<tokio::sync::broadcast::Receiver<crate::connection::ConnectionEvent>> {
        None
    }
}

#[tokio::test]
async fn test_pcm_streamer_retransmit() {
    use std::time::Duration;

    use crate::audio::{ChannelConfig, SampleFormat, SampleRate};
    use crate::streaming::source::SliceSource;

    let format = AudioFormat {
        sample_rate: SampleRate::Hz44100,
        channels: ChannelConfig::Stereo,
        sample_format: SampleFormat::I16,
    };
    let packets = Arc::new(Mutex::new(Vec::new()));
    let control_packets = Arc::new(Mutex::new(Vec::new()));
    let sender = Arc::new(MockRtpSender {
        packets: packets.clone(),
        control_packets: control_packets.clone(),
    });

    let streamer = PcmStreamer::new(sender, format, 44100);

    let data = vec![0u8; 352 * 4 * 1000]; // 10 packets worth
    let source = SliceSource::new(data, format);

    let streamer_clone = Arc::new(streamer);
    let streamer_task = streamer_clone.clone();

    // Start streaming in background
    let handle = tokio::spawn(async move {
        streamer_task.stream(source).await.unwrap();
    });

    // Wait for packets to be sent
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Request retransmit of sequence 0, count 2
    streamer_clone.retransmit(0, 2).await.unwrap();

    // Wait for retransmit to be processed
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify retransmit packets were sent
    let sent = control_packets.lock().unwrap().clone();
    assert_eq!(sent.len(), 2, "Should have sent 2 retransmit packets");

    // Retransmit packets should start with 0x80 0xD6
    assert_eq!(sent[0][0], 0x80);
    assert_eq!(sent[0][1], 0xD6);

    streamer_clone.stop().await.unwrap();
    let _ = handle.await;
}

#[tokio::test]
async fn test_pcm_streamer_creation() {
    use std::sync::Arc;

    use crate::connection::ConnectionManager;
    use crate::types::AirPlayConfig;

    let config = AirPlayConfig::default();
    let connection = Arc::new(ConnectionManager::new(config));
    let format = AudioFormat::CD_QUALITY;

    let streamer = PcmStreamer::new(connection, format, 44100);
    assert_eq!(streamer.state().await, StreamerState::Idle);
}

#[tokio::test]
async fn test_streaming_loop() {
    let sender = Arc::new(MockRtpSender::default());
    let packets = sender.packets.clone();

    let format = AudioFormat::CD_QUALITY;
    let streamer = PcmStreamer::new(sender, format, 44100);

    // Create source
    // Increase size to ensure it doesn't finish before we check state
    // 200,000 bytes at 44.1kHz stereo 16-bit (176,400 bytes/sec) is > 1 second
    let data = vec![1u8; 200_000];
    let source = SliceSource::new(data, format);

    // Start streaming in background
    let streamer_arc = Arc::new(streamer);
    let s = streamer_arc.clone();

    let handle = tokio::spawn(async move { s.stream(source).await });

    // Allow some time for streaming
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Check state (might be Streaming or finished if fast, but with interval it should be
    // streaming)
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
async fn benchmark_pcm_streaming_performance() {
    use std::sync::Arc;

    use crate::audio::AudioFormat;
    use crate::streaming::PcmStreamer;
    use crate::streaming::source::SliceSource;

    // Pause time to run fast
    tokio::time::pause();

    let sender = Arc::new(MockRtpSender::default());
    let format = AudioFormat::CD_QUALITY;
    let streamer = PcmStreamer::new(sender, format, 44100);

    // Create a large source
    // 352 frames * 4 bytes = 1408 bytes per packet
    // Let's process 10,000 packets => ~14MB
    let packet_size = 352 * 4;
    let num_packets = 10_000;
    let data = vec![0u8; packet_size * num_packets];
    let source = SliceSource::new(data, format);

    let start = std::time::Instant::now();
    streamer.stream(source).await.unwrap();
    let duration = start.elapsed();

    println!("Processed {num_packets} packets in {duration:?}");
}

#[tokio::test]
async fn test_finished_state() {
    let sender = Arc::new(MockRtpSender::default());
    let format = AudioFormat::CD_QUALITY;
    let streamer = PcmStreamer::new(sender, format, 44100);

    // Small source
    let data = vec![1u8; 1408 * 2]; // 2 packets
    let source = SliceSource::new(data, format);

    streamer.stream(source).await.unwrap();

    assert_eq!(streamer.state().await, StreamerState::Finished);
}

#[tokio::test]
async fn test_alac_encoding_usage() {
    let sender = Arc::new(MockRtpSender::default());
    let packets = sender.packets.clone();

    let format = AudioFormat::CD_QUALITY;
    let streamer = PcmStreamer::new(sender, format, 44100);

    // Enable ALAC
    streamer.use_alac().await;

    // Source data (silence compresses well)
    let data = vec![0u8; 1408 * 10];
    let source = SliceSource::new(data, format);

    streamer.stream(source).await.unwrap();

    let sent = packets.lock().unwrap();
    assert!(!sent.is_empty());
    // ALAC packets for silence should be smaller than 1408 bytes
    // (1408 bytes PCM -> ALAC header + small payload)
    for packet in sent.iter() {
        // RTP header + payload.
        // If it's pure RTP payload we mock send, it depends on RtpCodec.
        // But PcmStreamer calls connection.send_rtp_audio(packet).
        // RtpCodec adds header (12 bytes).
        // PCM payload is 1408. So total 1420.
        // ALAC silence should be much smaller.
        assert!(
            packet.len() < 1400,
            "Packet too large for ALAC silence: {}",
            packet.len()
        );
    }
}

#[tokio::test]
async fn test_resampling_integration() {
    use crate::audio::{ChannelConfig, SampleFormat, SampleRate};

    let sender = Arc::new(MockRtpSender::default());
    let packets = sender.packets.clone();

    let target_format = AudioFormat::CD_QUALITY;
    let streamer = PcmStreamer::new(sender, target_format, 44100);

    // Source is 48kHz
    let source_format = AudioFormat {
        sample_rate: SampleRate::Hz48000,
        channels: ChannelConfig::Stereo,
        sample_format: SampleFormat::I16,
    };

    // 100ms of audio
    let duration_secs = 0.1;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Test data generation"
    )]
    let num_samples = (f64::from(source_format.sample_rate.as_u32()) * duration_secs) as usize;
    let data = vec![0u8; num_samples * source_format.bytes_per_frame()];
    let source = SliceSource::new(data, source_format);

    // This should trigger resampling internally
    streamer.stream(source).await.unwrap();

    let sent = packets.lock().unwrap();
    assert!(!sent.is_empty());
    // Should still produce packets compatible with target format (44.1k)
    // We can't easily verify the content is resampled without decoding,
    // but we verify it ran without error and produced output.
}
