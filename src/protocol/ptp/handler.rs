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
    /// Address of the master.
    master_addr: SocketAddr,
    /// Next sequence ID for `Delay_Req`.
    delay_req_sequence: u16,
    /// Pending Sync T1 (from Sync or Follow-up).
    pending_t1: Option<PtpTimestamp>,
    /// T2 corresponding to pending T1.
    pending_t2: Option<PtpTimestamp>,
    /// Pending `Delay_Req` T3.
    pending_t3: Option<PtpTimestamp>,
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
            delay_req_sequence: 0,
            pending_t1: None,
            pending_t2: None,
            pending_t3: None,
        }
    }

    /// Run the slave handler loop.
    ///
    /// This spawns a task that:
    /// 1. Receives Sync messages and records T2
    /// 2. Receives Follow-up messages and records T1
    /// 3. Sends `Delay_Req` messages periodically (recording T3)
    /// 4. Receives `Delay_Resp` messages and records T4
    /// 5. Updates the PTP clock with complete measurements
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
                    self.handle_general_packet(&general_buf[..len], src);
                }

                // Send Delay_Req periodically (only if we have a Sync but no pending exchange).
                _ = delay_req_timer.tick() => {
                    if self.pending_t1.is_some() && self.pending_t3.is_none() {
                        self.send_delay_req().await?;
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
        _src: SocketAddr,
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
                    tracing::info!(
                        "PTP slave: Sync seq={}, two_step={}, T1={:?}",
                        msg.header.sequence_id,
                        two_step,
                        origin_timestamp
                    );
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
                _ => {
                    tracing::debug!("PTP slave: Ignoring event message type {:?}", msg.body);
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

    fn handle_general_packet(&mut self, data: &[u8], _src: SocketAddr) {
        if self.config.use_airplay_format {
            return;
        }

        match PtpMessage::decode(data) {
            Ok(msg) => {
                match msg.body {
                    PtpMessageBody::FollowUp {
                        precise_origin_timestamp,
                    } => {
                        tracing::info!(
                            "PTP slave: Follow_Up seq={}, T1={:?}",
                            msg.header.sequence_id,
                            precise_origin_timestamp
                        );
                        // Two-step Sync: the Follow-up carries the precise T1.
                        self.pending_t1 = Some(precise_origin_timestamp);
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

    async fn send_delay_req(&mut self) -> Result<(), std::io::Error> {
        let t3 = PtpTimestamp::now();
        self.pending_t3 = Some(t3);

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
            let msg = PtpMessage::delay_req(source, self.delay_req_sequence, t3);
            msg.encode()
        };

        tracing::info!(
            "PTP slave: Sending Delay_Req seq={} to {}",
            self.delay_req_sequence,
            self.master_addr
        );
        self.event_socket.send_to(&data, self.master_addr).await?;
        self.delay_req_sequence = self.delay_req_sequence.wrapping_add(1);
        Ok(())
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
                    Self::handle_general_message(&general_buf[..len], src);
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
                    tracing::debug!(
                        "PTP master: Received Sync from {} seq={}, two_step={}, clock=0x{:016X}, T1={}",
                        src,
                        msg.header.sequence_id,
                        two_step,
                        msg.header.source_port_identity.clock_identity,
                        origin_timestamp
                    );
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
    fn handle_general_message(data: &[u8], src: SocketAddr) {
        match PtpMessage::decode(data) {
            Ok(msg) => match &msg.body {
                PtpMessageBody::FollowUp {
                    precise_origin_timestamp,
                } => {
                    tracing::info!(
                        "PTP master: Received Follow_Up from {} seq={}, T1={}, clock=0x{:016X}",
                        src,
                        msg.header.sequence_id,
                        precise_origin_timestamp,
                        msg.header.source_port_identity.clock_identity
                    );
                }
                PtpMessageBody::Announce {
                    grandmaster_identity,
                    grandmaster_priority1,
                    ..
                } => {
                    tracing::info!(
                        "PTP master: Received Announce from {} seq={}, GM=0x{:016X}, priority1={}",
                        src,
                        msg.header.sequence_id,
                        grandmaster_identity,
                        grandmaster_priority1
                    );
                }
                PtpMessageBody::Signaling => {
                    tracing::debug!(
                        "PTP master: Received Signaling from {} seq={}",
                        src,
                        msg.header.sequence_id
                    );
                }
                _ => {
                    tracing::debug!(
                        "PTP master: Received {:?} on general port from {}",
                        msg.header.message_type,
                        src
                    );
                }
            },
            Err(e) => {
                let hex: Vec<String> = data.iter().take(20).map(|b| format!("{b:02X}")).collect();
                tracing::warn!(
                    "PTP master: Failed to decode general packet ({} bytes, first 20: [{}]): {}",
                    data.len(),
                    hex.join(", "),
                    e
                );
            }
        }
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
        // Remember this slave for future Sync broadcasts.
        self.add_slave(src);
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
        // Delay_Resp goes on general port if available.
        if let Some(ref general) = self.general_socket {
            general.send_to(&resp.encode(), src).await?;
        } else {
            self.event_socket.send_to(&resp.encode(), src).await?;
        }
        tracing::info!("PTP: Sent IEEE 1588 DelayResp to {}", src);

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
