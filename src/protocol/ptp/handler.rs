//! Async UDP handler for PTP timing exchanges.
//!
//! Provides both master (client/sender) and slave (receiver) handlers
//! for PTP timing over UDP. Standard PTP uses port 319 (event) and
//! port 320 (general), but `AirPlay` may use its own timing port.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::RwLock;

use super::clock::{PtpClock, PtpRole};
use super::message::{
    AirPlayTimingPacket, PtpMessage, PtpMessageBody, PtpMessageType, PtpPortIdentity,
};
use super::timestamp::PtpTimestamp;

/// Standard PTP event port (Sync, `Delay_Req`).
pub const PTP_EVENT_PORT: u16 = 319;

/// Standard PTP general port (`Follow_Up`, `Delay_Resp`, Announce).
pub const PTP_GENERAL_PORT: u16 = 320;

/// Configuration for PTP handler.
#[derive(Debug, Clone)]
pub struct PtpHandlerConfig {
    /// Clock identity for this endpoint.
    pub clock_id: u64,
    /// Role (master or slave).
    pub role: PtpRole,
    /// Interval between Sync messages when acting as master.
    pub sync_interval: Duration,
    /// Interval between `Delay_Req` messages when acting as slave.
    pub delay_req_interval: Duration,
    /// Maximum receive buffer size.
    pub recv_buf_size: usize,
    /// Use `AirPlay` compact packet format instead of IEEE 1588.
    pub use_airplay_format: bool,
}

impl Default for PtpHandlerConfig {
    fn default() -> Self {
        Self {
            clock_id: 0,
            role: PtpRole::Slave,
            sync_interval: Duration::from_secs(1),
            delay_req_interval: Duration::from_secs(1),
            recv_buf_size: 256,
            use_airplay_format: false,
        }
    }
}

/// Shared PTP clock state, accessible from multiple tasks.
pub type SharedPtpClock = Arc<RwLock<PtpClock>>;

/// PTP slave handler.
///
/// Listens for Sync/Follow-up from master, sends `Delay_Req`,
/// and processes `Delay_Resp` to synchronize the local clock.
pub struct PtpSlaveHandler {
    /// Event socket (port 319 or `AirPlay` timing port).
    event_socket: Arc<UdpSocket>,
    /// General socket (port 320), optional if using `AirPlay` format.
    general_socket: Option<Arc<UdpSocket>>,
    /// Shared clock state.
    clock: SharedPtpClock,
    /// Configuration.
    config: PtpHandlerConfig,
    /// Address of the master (event port 319).
    master_addr: SocketAddr,
    /// Optional alternative master address for `Delay_Req` (e.g., `ClockPorts`).
    master_clock_port_addr: Option<SocketAddr>,
    /// Next sequence ID for `Delay_Req`.
    delay_req_sequence: u16,
    /// Pending Sync T1 (from Sync or Follow-up).
    pending_t1: Option<PtpTimestamp>,
    /// T2 corresponding to pending T1.
    pending_t2: Option<PtpTimestamp>,
    /// Pending `Delay_Req` T3.
    pending_t3: Option<PtpTimestamp>,
    /// Pending `Delay_Resp` T4 received on general port.
    pending_delay_resp: Option<PtpTimestamp>,
    /// When the pending `Delay_Req` was sent (for timeout).
    delay_req_sent_at: Option<tokio::time::Instant>,
    /// Count of Sync messages processed (for one-way sync).
    sync_count: u64,
    /// Count of `Delay_Req` messages sent without response (for fallback logic).
    delay_req_no_resp_count: u32,
}

impl PtpSlaveHandler {
    /// Create a new slave handler.
    pub fn new(
        event_socket: Arc<UdpSocket>,
        general_socket: Option<Arc<UdpSocket>>,
        clock: SharedPtpClock,
        config: PtpHandlerConfig,
        master_addr: SocketAddr,
    ) -> Self {
        Self {
            event_socket,
            general_socket,
            clock,
            config,
            master_addr,
            master_clock_port_addr: None,
            delay_req_sequence: 0,
            pending_t1: None,
            pending_t2: None,
            pending_t3: None,
            pending_delay_resp: None,
            delay_req_sent_at: None,
            sync_count: 0,
            delay_req_no_resp_count: 0,
        }
    }

    /// Set an alternative address for `Delay_Req` (e.g., from `ClockPorts`).
    pub fn set_clock_port_addr(&mut self, addr: SocketAddr) {
        self.master_clock_port_addr = Some(addr);
    }

    /// Run the slave handler loop.
    ///
    /// This spawns a task that:
    /// 1. Receives Sync messages and records T2
    /// 2. Receives Follow-up messages and records T1
    /// 3. Sends `Delay_Req` messages periodically (recording T3)
    /// 4. Receives `Delay_Resp` messages and records T4
    /// 5. Updates the PTP clock with complete measurements
    /// 6. Falls back to one-way sync if `Delay_Resp` never arrives
    ///
    /// # Errors
    /// Returns `std::io::Error` if socket operations fail.
    pub async fn run(
        &mut self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), std::io::Error> {
        let mut event_buf = vec![0u8; self.config.recv_buf_size];
        let mut general_buf = vec![0u8; self.config.recv_buf_size];
        let mut delay_req_timer = tokio::time::interval(self.config.delay_req_interval);
        // Timeout for pending Delay_Req: if no Delay_Resp within 2 seconds, clear and retry.
        let delay_req_timeout = Duration::from_secs(2);
        // One-way sync: every N Sync messages without a Delay_Resp, do one-way sync.
        let one_way_sync_interval: u64 = 8;

        tracing::info!(
            "PTP slave: run loop starting, event_socket={:?}, general_socket={}",
            self.event_socket.local_addr(),
            self.general_socket
                .as_ref()
                .map_or("None".to_string(), |s| format!("{:?}", s.local_addr()))
        );

        loop {
            tokio::select! {
                // Receive on event socket.
                result = self.event_socket.recv_from(&mut event_buf) => {
                    let (len, src) = result?;
                    self.handle_event_packet(&event_buf[..len], src).await?;
                }

                // Receive on general socket (if available).
                result = async {
                    if let Some(ref sock) = self.general_socket {
                        sock.recv_from(&mut general_buf).await
                    } else {
                        // If no general socket, just pend forever.
                        std::future::pending().await
                    }
                } => {
                    let (len, src) = result?;
                    self.handle_general_packet(&general_buf[..len], src).await;
                    // Check if a Delay_Resp arrived on the general port and
                    // we have all four timestamps to complete a timing exchange.
                    self.try_complete_timing().await;
                }

                // Send Delay_Req periodically.
                _ = delay_req_timer.tick() => {
                    if self.sync_count == 0 && self.delay_req_sequence == 0 {
                        tracing::info!(
                            "PTP slave: Timer tick, waiting for first Sync (sync_count=0)"
                        );
                    }
                    // Check for timeout on pending Delay_Req.
                    if self.pending_t3.is_some() {
                        if let Some(sent_at) = self.delay_req_sent_at {
                            if sent_at.elapsed() > delay_req_timeout {
                                self.delay_req_no_resp_count += 1;
                                tracing::warn!(
                                    "PTP slave: Delay_Req timed out (no Delay_Resp after {:.1}s, count={})",
                                    sent_at.elapsed().as_secs_f64(),
                                    self.delay_req_no_resp_count
                                );
                                // Clear pending state so we can send a new Delay_Req.
                                self.pending_t3 = None;
                                self.delay_req_sent_at = None;
                            }
                        }
                    }

                    // Send Delay_Req if we have T1 from a Sync and no pending exchange.
                    if self.pending_t1.is_some() && self.pending_t3.is_none() {
                        self.send_delay_req().await?;
                    }

                    // One-way sync fallback: if we're getting Sync/Follow_Up but never
                    // Delay_Resp, use one-way estimates. One-way sync has no RTT correction
                    // but provides a rough offset which is better than nothing.
                    if self.sync_count > 0
                        && self.sync_count % one_way_sync_interval == 0
                        && self.delay_req_no_resp_count >= 3
                    {
                        self.try_one_way_sync().await;
                    }
                }

                // Shutdown signal.
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("PTP slave handler shutting down");
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_event_packet(
        &mut self,
        data: &[u8],
        src: SocketAddr,
    ) -> Result<(), std::io::Error> {
        let t2 = PtpTimestamp::now();

        if self.config.use_airplay_format {
            if let Ok(pkt) = AirPlayTimingPacket::decode(data) {
                match pkt.message_type {
                    PtpMessageType::Sync => {
                        self.pending_t1 = Some(pkt.timestamp);
                        self.pending_t2 = Some(t2);
                    }
                    PtpMessageType::DelayResp => {
                        if let (Some(t1), Some(t2_saved), Some(t3)) =
                            (self.pending_t1, self.pending_t2, self.pending_t3)
                        {
                            let t4 = pkt.timestamp;
                            let mut clock = self.clock.write().await;
                            clock.process_timing(t1, t2_saved, t3, t4);
                            self.pending_t1 = None;
                            self.pending_t2 = None;
                            self.pending_t3 = None;
                        }
                    }
                    _ => {}
                }
            }
        } else if let Ok(msg) = PtpMessage::decode(data) {
            match msg.body {
                PtpMessageBody::Sync { origin_timestamp } => {
                    let two_step = msg.header.flags & 0x0200 != 0;
                    self.sync_count += 1;
                    if self.sync_count <= 3 {
                        let hex: Vec<String> = data.iter().map(|b| format!("{b:02X}")).collect();
                        tracing::info!(
                            "PTP slave: Sync seq={}, two_step={}, T1={:?}, sync_count={}, \
                             src_clock=0x{:016X}, domain={}, hex=[{}]",
                            msg.header.sequence_id,
                            two_step,
                            origin_timestamp,
                            self.sync_count,
                            msg.header.source_port_identity.clock_identity,
                            msg.header.domain_number,
                            hex.join(" ")
                        );
                    } else if self.sync_count % 20 == 0 {
                        tracing::info!(
                            "PTP slave: Sync seq={}, sync_count={}",
                            msg.header.sequence_id,
                            self.sync_count,
                        );
                    }
                    // Always store T1 from Sync. For two-step, Follow_Up will
                    // overwrite with the precise value. If Follow_Up never arrives,
                    // this at least allows Delay_Req to be sent (keeping PTP alive).
                    self.pending_t1 = Some(origin_timestamp);
                    self.pending_t2 = Some(t2);
                }
                PtpMessageBody::FollowUp {
                    precise_origin_timestamp,
                } => {
                    // Follow_Up might arrive on event port in some implementations
                    tracing::info!(
                        "PTP slave: Follow_Up (on event port) seq={}, T1={:?}",
                        msg.header.sequence_id,
                        precise_origin_timestamp
                    );
                    self.pending_t1 = Some(precise_origin_timestamp);
                }
                PtpMessageBody::DelayResp {
                    receive_timestamp, ..
                } => {
                    tracing::info!("PTP slave: DelayResp T4={:?}", receive_timestamp);
                    // T4 = receive_timestamp from master.
                    if let (Some(t1), Some(t2_saved), Some(t3)) =
                        (self.pending_t1, self.pending_t2, self.pending_t3)
                    {
                        let t4 = receive_timestamp;
                        let mut clock = self.clock.write().await;
                        clock.process_timing(t1, t2_saved, t3, t4);
                        tracing::info!(
                            "PTP slave: Clock synced (offset={:.3}ms)",
                            clock.offset_millis()
                        );
                        self.pending_t1 = None;
                        self.pending_t2 = None;
                        self.pending_t3 = None;
                    }
                }
                other => {
                    tracing::info!(
                        "PTP slave: Unexpected event message type: {:?} from {}",
                        std::mem::discriminant(&other),
                        src
                    );
                }
            }
        } else {
            tracing::warn!(
                "PTP slave: Failed to decode event packet ({} bytes)",
                data.len()
            );
        }
        Ok(())
    }

    #[allow(
        clippy::unused_async,
        reason = "Async required for consistent trait/interface signature"
    )]
    async fn handle_general_packet(&mut self, data: &[u8], _src: SocketAddr) {
        if self.config.use_airplay_format {
            return;
        }

        match PtpMessage::decode(data) {
            Ok(msg) => {
                match msg.body {
                    PtpMessageBody::FollowUp {
                        precise_origin_timestamp,
                    } => {
                        if self.sync_count <= 3 || self.sync_count % 20 == 0 {
                            tracing::info!(
                                "PTP slave: Follow_Up seq={}, T1={:?}",
                                msg.header.sequence_id,
                                precise_origin_timestamp
                            );
                        }
                        // Two-step Sync: the Follow-up carries the precise T1.
                        self.pending_t1 = Some(precise_origin_timestamp);
                    }
                    PtpMessageBody::DelayResp {
                        receive_timestamp, ..
                    } => {
                        // Delay_Resp is a general message per IEEE 1588,
                        // so it arrives on port 320.
                        tracing::info!(
                            "PTP slave: DelayResp (general port) seq={}, T4={:?}",
                            msg.header.sequence_id,
                            receive_timestamp
                        );
                        self.pending_delay_resp = Some(receive_timestamp);
                    }
                    _ => {
                        tracing::debug!("PTP slave: Ignoring general message type {:?}", msg.body);
                    }
                }
            }
            Err(e) => {
                let hex: Vec<String> = data.iter().take(20).map(|b| format!("{b:02X}")).collect();
                tracing::warn!(
                    "PTP slave: Failed to decode general packet ({} bytes, first 20: [{}]): {:?}",
                    data.len(),
                    hex.join(", "),
                    e
                );
            }
        }
    }

    /// Try to complete a timing exchange if we have all four timestamps.
    ///
    /// This handles the case where `Delay_Resp` arrives on the general port
    /// (as per IEEE 1588) rather than the event port.
    async fn try_complete_timing(&mut self) {
        if let (Some(t1), Some(t2), Some(t3), Some(t4)) = (
            self.pending_t1,
            self.pending_t2,
            self.pending_t3,
            self.pending_delay_resp.take(),
        ) {
            let mut clock = self.clock.write().await;
            clock.process_timing(t1, t2, t3, t4);
            tracing::info!(
                "PTP slave: Clock synced via general port (offset={:.3}ms)",
                clock.offset_millis()
            );
            self.pending_t1 = None;
            self.pending_t2 = None;
            self.pending_t3 = None;
        }
    }

    async fn send_delay_req(&mut self) -> Result<(), std::io::Error> {
        let t3 = PtpTimestamp::now();
        self.pending_t3 = Some(t3);
        self.delay_req_sent_at = Some(tokio::time::Instant::now());

        let data = if self.config.use_airplay_format {
            let pkt = AirPlayTimingPacket {
                message_type: PtpMessageType::DelayReq,
                sequence_id: self.delay_req_sequence,
                timestamp: t3,
                clock_id: self.config.clock_id,
            };
            pkt.encode().to_vec()
        } else {
            let source = PtpPortIdentity::new(self.config.clock_id, 1);
            let mut msg = PtpMessage::delay_req(source, self.delay_req_sequence, t3);
            // Apple PTP uses transport_specific=1 (seen in HomePod's Sync messages).
            // Standard IEEE 1588 uses 0, but Apple devices may ignore Delay_Req
            // without this set.
            msg.header.transport_specific = 1;
            // Match Apple PTP flags: ptpTimescale (0x0008) + unicast (0x0400)
            // The HomePod Sync uses 0x0608 (twoStep+unicast+ptpTimescale).
            // For Delay_Req we don't set twoStep (0x0200).
            msg.header.flags = 0x0408; // unicast + ptpTimescale
            msg.encode()
        };

        // Send to standard event port (319).
        let target = self.master_addr;
        let hex: Vec<String> = data.iter().map(|b| format!("{b:02X}")).collect();
        tracing::info!(
            "PTP slave: Sending Delay_Req seq={} to {} ({} bytes): [{}]",
            self.delay_req_sequence,
            target,
            data.len(),
            hex.join(" ")
        );
        self.event_socket.send_to(&data, target).await?;

        // Also send to ClockPorts address if configured (HomePod may listen there).
        if let Some(clock_port_addr) = self.master_clock_port_addr {
            tracing::info!(
                "PTP slave: Also sending Delay_Req seq={} to ClockPorts addr {}",
                self.delay_req_sequence,
                clock_port_addr
            );
            self.event_socket.send_to(&data, clock_port_addr).await?;
        }

        self.delay_req_sequence = self.delay_req_sequence.wrapping_add(1);
        Ok(())
    }

    /// One-way sync fallback: estimate offset using only T1 and T2.
    ///
    /// When the master never responds to `Delay_Req`, we can still estimate
    /// the clock offset using one-way measurements. This assumes symmetric
    /// network delay, which gives a rough approximation.
    async fn try_one_way_sync(&mut self) {
        if let (Some(t1), Some(t2)) = (self.pending_t1, self.pending_t2) {
            // One-way offset = T2 - T1 (includes one-way network delay).
            // For actual PTP: offset = ((T2-T1) + (T3-T4))/2.
            // Without T3/T4, we just use T2 - T1 which includes network delay.
            // This is good enough for AirPlay rendering (tens of ms precision).
            let offset_nanos = t2.diff_nanos(&t1);
            #[allow(
                clippy::cast_precision_loss,
                reason = "Precision loss acceptable for millisecond display"
            )]
            let offset_ms = offset_nanos as f64 / 1_000_000.0;

            // Use T1/T2 for both halves of the exchange (treating T3=T2, T4=T1)
            // to get the clock.process_timing to accept it. The result is just (T2-T1).
            let mut clock = self.clock.write().await;
            clock.process_timing(t1, t2, t2, t1);
            tracing::info!(
                "PTP slave: One-way sync (no Delay_Resp), offset≈{:.3}ms, measurements={}",
                offset_ms,
                clock.measurement_count()
            );
        }
    }

    /// Get a handle to the shared clock.
    #[must_use]
    pub fn clock(&self) -> SharedPtpClock {
        self.clock.clone()
    }
}

/// PTP master handler.
///
/// Sends periodic Sync/Follow-up messages and responds to `Delay_Req`
/// with `Delay_Resp`. Used by the `AirPlay` client/sender.
pub struct PtpMasterHandler {
    /// Event socket.
    event_socket: Arc<UdpSocket>,
    /// General socket (for Follow-up, optional if using `AirPlay` format).
    general_socket: Option<Arc<UdpSocket>>,
    /// Shared clock state.
    clock: SharedPtpClock,
    /// Configuration.
    config: PtpHandlerConfig,
    /// Next Sync sequence ID.
    sync_sequence: u16,
    /// Known slave addresses (discovered from `Delay_Req` messages).
    known_slaves: Vec<SocketAddr>,
    /// Known slave general addresses (port 320) for `Follow_Up` messages.
    known_general_slaves: Vec<SocketAddr>,
    // --- Dual-role: also measure offset to remote clock ---
    /// Pending T1 from incoming `Sync/Follow_Up` (remote's timestamp).
    pending_remote_t1: Option<PtpTimestamp>,
    /// Pending T2 (our local time when we received the remote Sync).
    pending_remote_t2: Option<PtpTimestamp>,
    /// Pending T3 (our local time when we sent a `Delay_Req` to remote).
    pending_remote_t3: Option<PtpTimestamp>,
    /// Next `Delay_Req` sequence ID for dual-role measurements.
    delay_req_sequence: u16,
}

impl PtpMasterHandler {
    /// Create a new master handler.
    pub fn new(
        event_socket: Arc<UdpSocket>,
        general_socket: Option<Arc<UdpSocket>>,
        clock: SharedPtpClock,
        config: PtpHandlerConfig,
    ) -> Self {
        Self {
            event_socket,
            general_socket,
            clock,
            config,
            sync_sequence: 0,
            known_slaves: Vec::new(),
            known_general_slaves: Vec::new(),
            pending_remote_t1: None,
            pending_remote_t2: None,
            pending_remote_t3: None,
            delay_req_sequence: 0,
        }
    }

    /// Add a known slave event address (port 319) for Sync broadcasts.
    pub fn add_slave(&mut self, addr: SocketAddr) {
        if !self.known_slaves.contains(&addr) {
            self.known_slaves.push(addr);
        }
    }

    /// Add a known slave general address (port 320) for `Follow_Up` messages.
    pub fn add_general_slave(&mut self, addr: SocketAddr) {
        if !self.known_general_slaves.contains(&addr) {
            self.known_general_slaves.push(addr);
        }
    }

    /// Run the master handler loop.
    ///
    /// # Errors
    /// Returns `std::io::Error` if socket operations fail.
    pub async fn run(
        &mut self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), std::io::Error> {
        let mut event_buf = vec![0u8; self.config.recv_buf_size];
        let mut general_buf = vec![0u8; self.config.recv_buf_size];
        let mut sync_timer = tokio::time::interval(self.config.sync_interval);
        // Send Announce every 2 seconds
        let mut announce_timer = tokio::time::interval(Duration::from_secs(2));
        let mut announce_sequence: u16 = 0;
        // Dual-role: send Delay_Req to measure offset to the remote clock
        let mut delay_req_timer = tokio::time::interval(self.config.delay_req_interval);

        // Send initial Announce immediately
        self.send_announce(&mut announce_sequence).await?;

        loop {
            tokio::select! {
                // Receive on event socket (Sync, Delay_Req from HomePod).
                result = self.event_socket.recv_from(&mut event_buf) => {
                    let (len, src) = result?;
                    self.handle_event_message(&event_buf[..len], src).await?;
                }

                // Receive on general socket (Follow_Up, Announce, Signaling from HomePod).
                result = async {
                    if let Some(ref sock) = self.general_socket {
                        sock.recv_from(&mut general_buf).await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    let (len, src) = result?;
                    let first_byte = if len > 0 { format!("type=0x{:02X}", general_buf[0] & 0x0F) } else { "empty".to_string() };
                    tracing::info!("PTP master: Received {} bytes on general port from {} ({})", len, src, first_byte);
                    self.handle_general_message(&general_buf[..len], src).await;
                }

                // Send periodic Sync + Follow_Up to known slaves.
                _ = sync_timer.tick() => {
                    if self.known_slaves.is_empty() {
                        tracing::debug!("PTP: No known slaves yet, skipping Sync");
                    } else {
                        self.send_sync().await?;
                    }
                }

                // Send periodic Announce.
                _ = announce_timer.tick() => {
                    self.send_announce(&mut announce_sequence).await?;
                }

                // Dual-role: send Delay_Req to measure offset to remote clock.
                _ = delay_req_timer.tick() => {
                    if !self.known_slaves.is_empty() && !self.config.use_airplay_format {
                        self.send_delay_req_to_remote().await?;
                    }
                }

                // Shutdown.
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("PTP master handler shutting down");
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle incoming message on event port (319).
    async fn handle_event_message(
        &mut self,
        data: &[u8],
        src: SocketAddr,
    ) -> Result<(), std::io::Error> {
        if self.config.use_airplay_format {
            if let Ok(req) = AirPlayTimingPacket::decode(data) {
                if req.message_type == PtpMessageType::DelayReq {
                    return self.handle_airplay_delay_req(req, src).await;
                }
                tracing::debug!(
                    "PTP master: Received AirPlay message type {:?} from {} (ignored)",
                    req.message_type,
                    src
                );
                return Ok(());
            }
            return Ok(());
        }

        match PtpMessage::decode(data) {
            Ok(msg) => match &msg.body {
                PtpMessageBody::Sync { origin_timestamp } => {
                    let two_step = msg.header.flags & 0x0200 != 0;
                    let t2 = PtpTimestamp::now();
                    tracing::info!(
                        "PTP master: Received Sync from {} seq={}, two_step={}, clock=0x{:016X}, \
                         T1={}, T2={}",
                        src,
                        msg.header.sequence_id,
                        two_step,
                        msg.header.source_port_identity.clock_identity,
                        origin_timestamp,
                        t2
                    );
                    // Dual-role: record T2 for offset measurement.
                    self.pending_remote_t2 = Some(t2);
                    if !two_step {
                        // One-step: T1 is in the Sync itself.
                        self.pending_remote_t1 = Some(*origin_timestamp);
                    }
                }
                PtpMessageBody::DelayReq { .. } => {
                    tracing::info!(
                        "PTP master: Received Delay_Req from {} seq={}",
                        src,
                        msg.header.sequence_id
                    );
                    self.handle_ieee_delay_req(msg, src).await?;
                }
                _ => {
                    tracing::debug!(
                        "PTP master: Received {:?} on event port from {}",
                        msg.header.message_type,
                        src
                    );
                }
            },
            Err(e) => {
                let hex: Vec<String> = data.iter().take(20).map(|b| format!("{b:02X}")).collect();
                tracing::warn!(
                    "PTP master: Failed to decode event packet ({} bytes, first 20: [{}]): {}",
                    data.len(),
                    hex.join(", "),
                    e
                );
            }
        }
        Ok(())
    }

    /// Handle incoming message on general port (320).
    async fn handle_general_message(&mut self, data: &[u8], src: SocketAddr) {
        match PtpMessage::decode(data) {
            Ok(msg) => {
                tracing::info!(
                    "PTP master general: decoded {:?} from {} seq={} ({} bytes)",
                    msg.header.message_type,
                    src,
                    msg.header.sequence_id,
                    data.len()
                );
                match &msg.body {
                    PtpMessageBody::FollowUp {
                        precise_origin_timestamp,
                    } => {
                        tracing::info!(
                            "PTP master: Follow_Up from {} seq={}, T1={}, clock=0x{:016X}",
                            src,
                            msg.header.sequence_id,
                            precise_origin_timestamp,
                            msg.header.source_port_identity.clock_identity
                        );
                        // Dual-role: record precise T1 from remote's Follow_Up.
                        self.pending_remote_t1 = Some(*precise_origin_timestamp);
                    }
                    PtpMessageBody::DelayResp {
                        receive_timestamp, ..
                    } => {
                        // Dual-role: this is T4 from the remote responding to our Delay_Req.
                        tracing::info!(
                            "PTP master: Received Delay_Resp from {} seq={}, T4={}",
                            src,
                            msg.header.sequence_id,
                            receive_timestamp
                        );
                        // Complete the timing exchange with all four timestamps.
                        if let (Some(t1), Some(t2), Some(t3)) = (
                            self.pending_remote_t1.take(),
                            self.pending_remote_t2.take(),
                            self.pending_remote_t3.take(),
                        ) {
                            let t4 = *receive_timestamp;
                            let mut clock = self.clock.write().await;
                            clock.process_timing(t1, t2, t3, t4);
                            tracing::info!(
                                "PTP master: Clock synced with remote (offset={:.3}ms, \
                                 measurements={})",
                                clock.offset_millis(),
                                clock.measurement_count()
                            );
                        } else {
                            tracing::debug!(
                                "PTP master: Delay_Resp received but missing T1/T2/T3 — timing \
                                 incomplete"
                            );
                        }
                    }
                    PtpMessageBody::Announce {
                        grandmaster_identity,
                        grandmaster_priority1,
                        ..
                    } => {
                        tracing::debug!(
                            "PTP master: Received Announce from {} seq={}, GM=0x{:016X}, \
                             priority1={}",
                            src,
                            msg.header.sequence_id,
                            grandmaster_identity,
                            grandmaster_priority1
                        );
                    }
                    PtpMessageBody::Signaling => {
                        tracing::info!(
                            "PTP master: Received Signaling from {} seq={}",
                            src,
                            msg.header.sequence_id
                        );
                    }
                    _ => {
                        let hex: Vec<String> =
                            data.iter().take(44).map(|b| format!("{b:02X}")).collect();
                        tracing::info!(
                            "PTP master: Received {:?} ({} bytes) on general port from {} seq={}, \
                             hex=[{}]",
                            msg.header.message_type,
                            data.len(),
                            src,
                            msg.header.sequence_id,
                            hex.join(", ")
                        );
                    }
                }
            }
            Err(e) => {
                let hex: Vec<String> = data.iter().take(44).map(|b| format!("{b:02X}")).collect();
                tracing::warn!(
                    "PTP master: Failed to decode general packet ({} bytes, first 44: [{}]): {}",
                    data.len(),
                    hex.join(", "),
                    e
                );
            }
        }
    }

    /// Dual-role: send `Delay_Req` to the first known slave to measure clock offset.
    async fn send_delay_req_to_remote(&mut self) -> Result<(), std::io::Error> {
        // Send to the first known slave on event port.
        if let Some(&slave_addr) = self.known_slaves.first() {
            let t3 = PtpTimestamp::now();
            let source = PtpPortIdentity::new(self.config.clock_id, 1);
            let req = PtpMessage::delay_req(source, self.delay_req_sequence, t3);
            self.event_socket.send_to(&req.encode(), slave_addr).await?;
            self.pending_remote_t3 = Some(t3);
            tracing::info!(
                "PTP master: Sent Delay_Req seq={} to {}",
                self.delay_req_sequence,
                slave_addr
            );
            self.delay_req_sequence = self.delay_req_sequence.wrapping_add(1);
        }
        Ok(())
    }

    /// Send Announce message to establish ourselves as PTP master.
    async fn send_announce(&self, sequence: &mut u16) -> Result<(), std::io::Error> {
        let source = PtpPortIdentity::new(self.config.clock_id, 1);
        let announce = PtpMessage::announce(
            source,
            *sequence,
            self.config.clock_id, // grandmaster = ourselves
            128,                  // priority1 (lower = better, 128 = default; HomePod sends 248)
            128,                  // priority2
        );
        let encoded = announce.encode();
        if let Some(ref general) = self.general_socket {
            for &addr in &self.known_general_slaves {
                general.send_to(&encoded, addr).await?;
            }
            tracing::info!(
                "PTP master: Sent Announce seq={}, {} bytes, clock=0x{:016X}, priority1=128",
                *sequence,
                encoded.len(),
                self.config.clock_id
            );
        }
        *sequence = sequence.wrapping_add(1);
        Ok(())
    }

    async fn send_sync(&mut self) -> Result<(), std::io::Error> {
        let t1 = PtpTimestamp::now();
        let source = PtpPortIdentity::new(self.config.clock_id, 1);

        for &slave_addr in &self.known_slaves {
            if self.config.use_airplay_format {
                let pkt = AirPlayTimingPacket {
                    message_type: PtpMessageType::Sync,
                    sequence_id: self.sync_sequence,
                    timestamp: t1,
                    clock_id: self.config.clock_id,
                };
                self.event_socket.send_to(&pkt.encode(), slave_addr).await?;
            } else {
                // Two-step Sync: send Sync with approximate timestamp,
                // then Follow-up with precise timestamp.
                let mut sync_msg = PtpMessage::sync(source, self.sync_sequence, t1);
                sync_msg.header.flags = 0x0200; // Two-step flag
                self.event_socket
                    .send_to(&sync_msg.encode(), slave_addr)
                    .await?;
                tracing::debug!(
                    "PTP master: Sent Sync seq={} to {}",
                    self.sync_sequence,
                    slave_addr
                );

                // Precise timestamp (in practice, captured by hardware).
                let precise_t1 = PtpTimestamp::now();
                let follow_up = PtpMessage::follow_up(source, self.sync_sequence, precise_t1);
                if let Some(ref general) = self.general_socket {
                    // Send Follow_Up to general port addresses (port 320)
                    for &general_addr in &self.known_general_slaves {
                        general.send_to(&follow_up.encode(), general_addr).await?;
                        tracing::debug!(
                            "PTP master: Sent Follow_Up seq={} to {}",
                            self.sync_sequence,
                            general_addr
                        );
                    }
                    // Fallback: also send to slave event addr if no general slaves
                    if self.known_general_slaves.is_empty() {
                        general.send_to(&follow_up.encode(), slave_addr).await?;
                    }
                } else {
                    self.event_socket
                        .send_to(&follow_up.encode(), slave_addr)
                        .await?;
                }
            }
        }

        self.sync_sequence = self.sync_sequence.wrapping_add(1);
        Ok(())
    }

    async fn handle_airplay_delay_req(
        &mut self,
        req: AirPlayTimingPacket,
        src: SocketAddr,
    ) -> Result<(), std::io::Error> {
        // Remember this slave for future Sync broadcasts.
        self.add_slave(src);
        let t4 = PtpTimestamp::now();

        tracing::info!(
            "PTP: AirPlay format message type={:?}, seq={}",
            req.message_type,
            req.sequence_id
        );

        let resp = AirPlayTimingPacket {
            message_type: PtpMessageType::DelayResp,
            sequence_id: req.sequence_id,
            timestamp: t4,
            clock_id: self.config.clock_id,
        };
        self.event_socket.send_to(&resp.encode(), src).await?;
        tracing::info!("PTP: Sent AirPlay DelayResp to {}", src);

        Ok(())
    }

    async fn handle_ieee_delay_req(
        &mut self,
        msg: PtpMessage,
        src: SocketAddr,
    ) -> Result<(), std::io::Error> {
        // Remember this slave for future Sync broadcasts (event port).
        self.add_slave(src);

        // Also register the general port address for Follow_Up messages.
        // IEEE 1588: general messages (Follow_Up, Delay_Resp) use port 320.
        let general_addr = SocketAddr::new(src.ip(), PTP_GENERAL_PORT);
        self.add_general_slave(general_addr);

        let t4 = PtpTimestamp::now();
        let source = PtpPortIdentity::new(self.config.clock_id, 1);

        tracing::info!(
            "PTP: IEEE 1588 message type={:?}, seq={}",
            msg.body,
            msg.header.sequence_id
        );

        let resp = PtpMessage::delay_resp(
            source,
            msg.header.sequence_id,
            t4,
            msg.header.source_port_identity,
        );
        // Delay_Resp is a general message — send to general port (320), not event port (319).
        if let Some(ref general) = self.general_socket {
            general.send_to(&resp.encode(), general_addr).await?;
        } else {
            // Fallback: no general socket, send on event socket to original source.
            self.event_socket.send_to(&resp.encode(), src).await?;
        }
        tracing::info!("PTP: Sent IEEE 1588 DelayResp to {}", general_addr);

        Ok(())
    }

    /// Get a handle to the shared clock.
    #[must_use]
    pub fn clock(&self) -> SharedPtpClock {
        self.clock.clone()
    }
}

/// Create a shared PTP clock instance.
#[must_use]
pub fn create_shared_clock(clock_id: u64, role: PtpRole) -> SharedPtpClock {
    Arc::new(RwLock::new(PtpClock::new(clock_id, role)))
}
