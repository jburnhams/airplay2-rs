1. **Update `UdpSockets` to use `Arc<UdpSocket>` for `control`**
   - Modify `src/connection/manager.rs`: `UdpSockets { pub(crate) control: Arc<UdpSocket>, ... }`
   - Adjust `ConnectionManager` where `UdpSockets` is instantiated.

2. **Add `send_rtcp_control` to `RtpSender`**
   - In `src/streaming/pcm.rs`, add `async fn send_rtcp_control(&self, packet: &[u8]) -> Result<(), AirPlayError>` to `RtpSender`.
   - In `src/connection/manager.rs`, add `pub async fn send_rtcp_control` that uses `self.sockets` to send the payload to `server_control_port`.

3. **Listen for RetransmitRequests in `ConnectionManager`**
   - In `ConnectionManager::setup_session` (around where `self.sockets` is populated), spawn a tokio task that loops `recv_from` on the `control` socket.
   - Parse RTCP packets (specifically Type 213 / 0xD5 for `RetransmitRequest`).
   - On receiving a `RetransmitRequest`, emit `ConnectionEvent::RetransmitRequest { seq_start: u16, count: u16 }` over `self.event_tx`.

4. **Add `RetransmitRequest` to `ConnectionEvent`**
   - Update `src/connection/state.rs` and any match statements that need a `_ => {}` arm.

5. **Forward Event to Streamer**
   - In `AirPlayClient::start_monitor` (`src/client/mod.rs`), handle `ConnectionEvent::RetransmitRequest` and call `self.streamer.retransmit(seq_start, count)` if `streamer` is some.

6. **Add `PacketBuffer` to `PcmStreamer`**
   - Import and use `PacketBuffer::new` in `PcmStreamer::new()`.
   - When a packet is encoded in `PcmStreamer::stream` (just before sending), extract `sequence` and `timestamp` from the RTP header (e.g. `u16::from_be_bytes`), create a `BufferedPacket`, and add it to `PacketBuffer`.
   - Ensure the RTP header byte indices are correct: `sequence` at offset 2-3, `timestamp` at offset 4-7.

7. **Handle Retransmission in `PcmStreamer`**
   - Add `Retransmit(u16, u16)` to `StreamerCommand`.
   - Implement `pub async fn retransmit(&self, seq: u16, count: u16)` pushing to `self.cmd_tx`.
   - In the `stream` loop, handle `Retransmit(seq_start, count)`:
     - Iterate through `self.packet_buffer.get_range(seq_start, count)` (needs mutable access to `PacketBuffer` so put it in a `Mutex` or make it mutable in the loop).
     - For each missing packet, construct a `RetransmitResponse` (RTCP header `0x80, 0xD6, length_hi, length_lo` followed by the full original RTP packet).
     - Send the assembled response via `self.connection.send_rtcp_control()`.

8. **Tests and Verification**
   - Ensure we update `airplay2-checklist.md` noting the checklist completed and verified logic. (Wait, the check needs to pass the precommit and python receiver test. The python receiver will naturally send `0xD5` if we simulate packet loss. But I can just write an integration test like `raop_streaming_integration.rs` or `reconnection_integration.rs` that verifies `packet_buffer` behavior or relies on `network_flakiness.rs`).
   - Add a test or just rely on existing to ensure it builds.
   - Run `cargo clippy` and `cargo fmt`.
