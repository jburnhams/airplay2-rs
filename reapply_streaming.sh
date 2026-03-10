#!/bin/bash
cat << 'PATCH' > src/streaming/pcm_patch.diff
--- src/streaming/pcm.rs
+++ src/streaming/pcm.rs
@@ -27,6 +27,14 @@
         rtp_timestamp: u32,
         sample_rate: u32,
     ) -> Result<(), AirPlayError>;
+
+    /// Send RTCP control packet (e.g., RetransmitResponse)
+    async fn send_rtcp_control(&self, packet: &[u8]) -> Result<(), AirPlayError>;
+
+    /// Subscribe to connection events
+    fn subscribe_events(
+        &self,
+    ) -> Option<tokio::sync::broadcast::Receiver<crate::connection::ConnectionEvent>>;
 }

 #[async_trait]
@@ -42,6 +50,16 @@
     ) -> Result<(), AirPlayError> {
         self.send_time_announce(rtp_timestamp, sample_rate).await
     }
+
+    async fn send_rtcp_control(&self, packet: &[u8]) -> Result<(), AirPlayError> {
+        self.send_rtcp_control(packet).await
+    }
+
+    fn subscribe_events(
+        &self,
+    ) -> Option<tokio::sync::broadcast::Receiver<crate::connection::ConnectionEvent>> {
+        Some(self.subscribe())
+    }
 }

 /// PCM streamer state
@@ -104,6 +122,8 @@
     encoder_aac: Mutex<Option<AacEncoder>>,
     /// Codec type
     codec_type: RwLock<AudioCodec>,
+    /// Outgoing packet buffer for retransmissions
+    packet_buffer: Mutex<crate::protocol::rtp::packet_buffer::PacketBuffer>,
 }

 /// Commands for the streamer
@@ -119,6 +139,8 @@
     Stop,
     /// Seek to position
     Seek(Duration),
+    /// Retransmit packets
+    Retransmit(u16, u16),
 }

 impl PcmStreamer {
@@ -153,6 +175,9 @@
             encoder: Mutex::new(None),
             encoder_aac: Mutex::new(None),
             codec_type: RwLock::new(AudioCodec::Pcm),
+            packet_buffer: Mutex::new(crate::protocol::rtp::packet_buffer::PacketBuffer::new(
+                crate::protocol::rtp::packet_buffer::PacketBuffer::DEFAULT_SIZE,
+            )),
         }
     }

@@ -421,6 +446,24 @@
                     // Send packet
                     self.send_packet(&rtp_packet_buffer).await?;
                     packets_sent += 1;
+
+                    // Buffer packet for retransmissions
+                    if rtp_packet_buffer.len() >= 12 {
+                        let seq = u16::from_be_bytes([rtp_packet_buffer[2], rtp_packet_buffer[3]]);
+                        let ts = u32::from_be_bytes([
+                            rtp_packet_buffer[4],
+                            rtp_packet_buffer[5],
+                            rtp_packet_buffer[6],
+                            rtp_packet_buffer[7],
+                        ]);
+                        self.packet_buffer
+                            .lock()
+                            .await
+                            .push(crate::protocol::rtp::packet_buffer::BufferedPacket {
+                                sequence: seq,
+                                timestamp: ts,
+                                data: bytes::Bytes::copy_from_slice(&rtp_packet_buffer),
+                            });
+                    }
                     if packets_sent == 1 {
                         tracing::info!(
@@ -522,6 +565,30 @@
                                 self.fill_buffer(&mut source)?;
                             }
                         }
+                        Some(StreamerCommand::Retransmit(seq_start, count)) => {
+                            let packets_to_send: Vec<Vec<u8>> = {
+                                let buffer = self.packet_buffer.lock().await;
+                                buffer
+                                    .get_range(seq_start, count)
+                                    .map(|p| {
+                                        // Retransmit response is [0x80, 0xD6, length_hi, length_lo, ...original packet]
+                                        let len_words = (p.data.len() / 4) as u16;
+                                        let mut response = Vec::with_capacity(4 + p.data.len());
+                                        response.push(0x80);
+                                        response.push(0xD6);
+                                        response.extend_from_slice(&len_words.to_be_bytes());
+                                        response.extend_from_slice(&p.data);
+                                        response
+                                    })
+                                    .collect()
+                            };
+
+                            for pkt in packets_to_send {
+                                if let Err(e) = self.connection.send_rtcp_control(&pkt).await {
+                                    tracing::warn!("Failed to send retransmit packet: {e}");
+                                }
+                            }
+                        }
                         None => {
                             // Channel closed
                             tracing::debug!("Command channel closed, stopping streamer");
@@ -566,6 +633,17 @@
                 current_state: "unknown".to_string(),
             })
     }
+
+    /// Retransmit lost packets
+    pub async fn retransmit(&self, seq_start: u16, count: u16) -> Result<(), AirPlayError> {
+        self.cmd_tx
+            .send(StreamerCommand::Retransmit(seq_start, count))
+            .await
+            .map_err(|_| AirPlayError::InvalidState {
+                message: "Streamer not running".to_string(),
+                current_state: "unknown".to_string(),
+            })
+    }

     /// Resume streaming
PATCH
patch src/streaming/pcm.rs < src/streaming/pcm_patch.diff || echo "Failed to patch pcm.rs"
