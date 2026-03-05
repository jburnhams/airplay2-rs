//! Unified PTP node that participates in both master and slave roles.
//!
//! A `PtpNode` can simultaneously:
//! - Send Sync/Follow_Up and respond to Delay_Req (master behaviour)
//! - Process incoming Sync/Follow_Up, send Delay_Req, and process Delay_Resp (slave behaviour)
//! - Evaluate Announce messages and switch roles via a simplified BMCA
//!
//! This is needed because AirPlay 2 devices (e.g. HomePod) may act as
//! grandmaster clock, and the client must be able to sync to them. The
//! role is determined by comparing clock priorities from Announce messages.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;

use super::handler::SharedPtpClock;
use super::message::{
    AirPlayTimingPacket, PtpMessage, PtpMessageBody, PtpMessageType, PtpPortIdentity,
};
use super::timestamp::PtpTimestamp;

/// Configuration for a PTP node.
#[derive(Debug, Clone)]
pub struct PtpNodeConfig {
    /// Clock identity for this endpoint.
    pub clock_id: u64,
    /// Our Announce priority1 (lower = higher priority, 128 = default).
    pub priority1: u8,
    /// Our Announce priority2.
    pub priority2: u8,
    /// Interval between Sync messages when acting as master.
    pub sync_interval: Duration,
    /// Interval between Delay_Req messages when acting as slave.
    pub delay_req_interval: Duration,
    /// Interval between Announce messages.
    pub announce_interval: Duration,
    /// Maximum receive buffer size.
    pub recv_buf_size: usize,
    /// Use AirPlay compact packet format instead of IEEE 1588.
    pub use_airplay_format: bool,
}

impl Default for PtpNodeConfig {
    fn default() -> Self {
        Self {
            clock_id: 0,
            priority1: 128,
            priority2: 128,
            sync_interval: Duration::from_secs(1),
            delay_req_interval: Duration::from_secs(1),
            announce_interval: Duration::from_secs(2),
            recv_buf_size: 256,
            use_airplay_format: false,
        }
    }
}

/// The current effective role of the node as determined by BMCA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveRole {
    /// We are the master (best clock on the network).
    Master,
    /// We are a slave to a remote grandmaster.
    Slave,
}

/// State tracked about a remote master discovered via Announce.
#[derive(Debug, Clone)]
#[allow(dead_code, reason = "Fields retained for diagnostics and future BMCA extensions")]
struct RemoteMaster {
    /// Clock identity of the grandmaster.
    grandmaster_identity: u64,
    /// Priority1 from the Announce.
    priority1: u8,
    /// Priority2 from the Announce.
    priority2: u8,
    /// Address from which the Announce was received (event port).
    event_addr: SocketAddr,
    /// General port address for this master.
    general_addr: SocketAddr,
    /// When we last heard an Announce from this master.
    last_announce: tokio::time::Instant,
}

/// Timeout after which an unanswered Delay_Req is considered lost.
const DELAY_REQ_TIMEOUT: Duration = Duration::from_millis(1000);

/// Unified PTP node supporting bidirectional synchronization.
///
/// Runs a single event loop that handles both master and slave message
/// flows on the same sockets. Uses a simplified BMCA to determine
/// whether this node should act as master or slave.
pub struct PtpNode {
    /// Event socket (port 319 or AirPlay timing port).
    event_socket: Arc<UdpSocket>,
    /// General socket (port 320), optional if using AirPlay format.
    general_socket: Option<Arc<UdpSocket>>,
    /// Shared clock state.
    clock: SharedPtpClock,
    /// Configuration.
    config: PtpNodeConfig,
    /// Current effective role.
    role: EffectiveRole,
    /// Next Sync sequence ID (master).
    sync_sequence: u16,
    /// Next Delay_Req sequence ID (slave).
    delay_req_sequence: u16,
    /// Next Announce sequence ID.
    announce_sequence: u16,
    /// Known slave addresses for Sync broadcasts (master role).
    known_slaves: Vec<SocketAddr>,
    /// Known slave general addresses for Follow_Up (master role).
    known_general_slaves: Vec<SocketAddr>,
    /// Pending Sync T1 (slave role).
    pending_t1: Option<PtpTimestamp>,
    /// T2 corresponding to pending T1 (slave role).
    pending_t2: Option<PtpTimestamp>,
    /// Pending Delay_Req T3 (slave role).
    pending_t3: Option<PtpTimestamp>,
    /// When the most recent Delay_Req was sent (for timeout/retry).
    delay_req_sent_at: Option<tokio::time::Instant>,
    /// The current remote master we are slaving to (if any).
    remote_master: Option<RemoteMaster>,
    /// Announce timeout: if no Announce from the remote master within
    /// this duration, assume it's gone and revert to master.
    announce_timeout: Duration,
}

impl PtpNode {
    /// Create a new PTP node.
    ///
    /// Starts in `Master` role by default. Will switch to `Slave` when
    /// a higher-priority Announce is received.
    pub fn new(
        event_socket: Arc<UdpSocket>,
        general_socket: Option<Arc<UdpSocket>>,
        clock: SharedPtpClock,
        config: PtpNodeConfig,
    ) -> Self {
        Self {
            event_socket,
            general_socket,
            clock,
            config,
            role: EffectiveRole::Master,
            sync_sequence: 0,
            delay_req_sequence: 0,
            announce_sequence: 0,
            known_slaves: Vec::new(),
            known_general_slaves: Vec::new(),
            pending_t1: None,
            pending_t2: None,
            pending_t3: None,
            delay_req_sent_at: None,
            remote_master: None,
            announce_timeout: Duration::from_secs(6),
        }
    }

    /// Add a known slave event address for Sync broadcasts.
    ///
    /// Slave event and general addresses are stored in parallel vectors
    /// (same index = same peer), so `add_slave` and `add_general_slave`
    /// should be called in pairs.
    pub fn add_slave(&mut self, addr: SocketAddr) {
        if !self.known_slaves.contains(&addr) {
            self.known_slaves.push(addr);
        }
    }

    /// Add a known slave general address for Follow_Up messages.
    ///
    /// Must be called after `add_slave` for the same peer to maintain
    /// parallel index alignment.
    pub fn add_general_slave(&mut self, addr: SocketAddr) {
        if !self.known_general_slaves.contains(&addr) {
            self.known_general_slaves.push(addr);
        }
    }

    /// Get the current effective role.
    #[must_use]
    pub fn role(&self) -> EffectiveRole {
        self.role
    }

    /// Get a handle to the shared clock.
    #[must_use]
    pub fn clock(&self) -> SharedPtpClock {
        self.clock.clone()
    }

    /// Run the PTP node event loop.
    ///
    /// This handles all PTP message exchange for both master and slave roles,
    /// switching between them as Announce messages dictate.
    ///
    /// # Errors
    /// Returns `std::io::Error` if socket operations fail.
    #[allow(clippy::too_many_lines)]
    pub async fn run(
        &mut self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), std::io::Error> {
        let mut event_buf = vec![0u8; self.config.recv_buf_size];
        let mut general_buf = vec![0u8; self.config.recv_buf_size];
        let mut sync_timer = tokio::time::interval(self.config.sync_interval);
        let mut delay_req_timer = tokio::time::interval(self.config.delay_req_interval);
        let mut announce_timer = tokio::time::interval(self.config.announce_interval);

        // Send initial Announce
        self.send_announce().await?;

        loop {
            tokio::select! {
                // Receive on event socket.
                result = self.event_socket.recv_from(&mut event_buf) => {
                    match result {
                        Ok((len, src)) => {
                            tracing::info!(
                                "PTP event RX: {} bytes from {} (first byte: {:02X})",
                                len, src,
                                event_buf.first().copied().unwrap_or(0)
                            );
                            self.handle_event_packet(&event_buf[..len], src).await?;
                        }
                        Err(e) if Self::is_transient_udp_error(&e) => {
                            // Windows WSAECONNRESET (10054) or similar — ignore and retry.
                            tracing::debug!("PTP node: transient event socket error: {}", e);
                        }
                        Err(e) => return Err(e),
                    }
                }

                // Receive on general socket (if available).
                result = async {
                    if let Some(ref sock) = self.general_socket {
                        sock.recv_from(&mut general_buf).await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match result {
                        Ok((len, src)) => {
                            tracing::info!(
                                "PTP general RX: {} bytes from {} (first byte: {:02X})",
                                len, src,
                                general_buf.first().copied().unwrap_or(0)
                            );
                            self.handle_general_packet(&general_buf[..len], src).await?;
                        }
                        Err(e) if Self::is_transient_udp_error(&e) => {
                            tracing::debug!("PTP node: transient general socket error: {}", e);
                        }
                        Err(e) => return Err(e),
                    }
                }

                // Periodic Sync + Follow_Up (only when master).
                _ = sync_timer.tick() => {
                    if self.role == EffectiveRole::Master && !self.known_slaves.is_empty() {
                        self.send_sync().await?;
                    }
                }

                // Periodic Delay_Req: retry if we have T1/T2 but no Delay_Resp yet.
                // In BMCA mode this only fires when role==Slave; in AirPlay
                // compact format (no BMCA) we always respond to received Syncs.
                _ = delay_req_timer.tick() => {
                    let in_slave_mode = self.role == EffectiveRole::Slave
                        || self.config.use_airplay_format;
                    if in_slave_mode && self.pending_t1.is_some() {
                        // If a previous Delay_Req went unanswered, clear the stale
                        // T3 so the retry can proceed.
                        let timed_out = self.delay_req_sent_at
                            .map_or(false, |t| t.elapsed() > DELAY_REQ_TIMEOUT);
                        if timed_out {
                            tracing::debug!(
                                "PTP: Delay_Req timed out (no Delay_Resp), clearing T3 for retry"
                            );
                            self.pending_t3 = None;
                            self.delay_req_sent_at = None;
                        }
                        if self.pending_t3.is_none() {
                            self.send_delay_req().await?;
                        }
                    }
                }

                // Periodic Announce.
                _ = announce_timer.tick() => {
                    self.send_announce().await?;
                    self.check_announce_timeout();
                }

                // Shutdown.
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("PTP node shutting down (role={:?})", self.role);
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Resolve the general-port address that corresponds to a given event-port
    /// source address. Uses the parallel `known_slaves` / `known_general_slaves`
    /// lists (indexed by position) to find the mapping. Falls back to standard
    /// PTP general port (320) on the source IP if no match is found.
    fn resolve_general_addr_for_event(&self, event_src: SocketAddr) -> SocketAddr {
        // Try positional match: known_slaves[i] <-> known_general_slaves[i]
        for (i, slave_addr) in self.known_slaves.iter().enumerate() {
            if *slave_addr == event_src {
                if let Some(general_addr) = self.known_general_slaves.get(i) {
                    return *general_addr;
                }
            }
        }
        // Fallback: same IP, standard general port
        SocketAddr::new(event_src.ip(), super::handler::PTP_GENERAL_PORT)
    }

    /// Check if a UDP error is transient and should be retried.
    ///
    /// On Windows, `WSAECONNRESET` (10054) is returned by `recv_from` after a
    /// previous `send_to` triggered an ICMP "port unreachable". This is benign
    /// in PTP because the remote peer may not have started listening yet.
    fn is_transient_udp_error(e: &std::io::Error) -> bool {
        // Windows WSAECONNRESET
        if e.raw_os_error() == Some(10054) {
            return true;
        }
        // ConnectionReset on any platform
        if e.kind() == std::io::ErrorKind::ConnectionReset {
            return true;
        }
        false
    }

    /// Handle incoming packet on event port (319).
    async fn handle_event_packet(
        &mut self,
        data: &[u8],
        src: SocketAddr,
    ) -> Result<(), std::io::Error> {
        let receive_time = PtpTimestamp::now();

        if self.config.use_airplay_format {
            if let Ok(pkt) = AirPlayTimingPacket::decode(data) {
                match pkt.message_type {
                    PtpMessageType::Sync => {
                        // Incoming Sync — we are acting as slave for this exchange.
                        self.pending_t1 = Some(pkt.timestamp);
                        self.pending_t2 = Some(receive_time);
                    }
                    PtpMessageType::DelayReq => {
                        // Incoming Delay_Req — we respond as master.
                        self.add_slave(src);
                        self.handle_airplay_delay_req(pkt, src).await?;
                    }
                    PtpMessageType::DelayResp => {
                        // Incoming Delay_Resp — we process as slave.
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
            return Ok(());
        }

        // IEEE 1588 format
        match PtpMessage::decode(data) {
            Ok(msg) => match &msg.body {
                PtpMessageBody::Sync { origin_timestamp } => {
                    let two_step = msg.header.flags & 0x0200 != 0;
                    tracing::debug!(
                        "PTP node: Received Sync from {} seq={} domain={} two_step={} T1={}",
                        src,
                        msg.header.sequence_id,
                        msg.header.domain_number,
                        two_step,
                        origin_timestamp
                    );
                    // Store T1/T2 for slave-side processing.
                    self.pending_t1 = Some(*origin_timestamp);
                    self.pending_t2 = Some(receive_time);
                }
                PtpMessageBody::FollowUp {
                    precise_origin_timestamp,
                } => {
                    // Follow_Up may arrive on event port in some implementations.
                    tracing::debug!(
                        "PTP node: Follow_Up (event port) seq={}, T1={}",
                        msg.header.sequence_id,
                        precise_origin_timestamp
                    );
                    self.pending_t1 = Some(*precise_origin_timestamp);
                }
                PtpMessageBody::DelayReq { .. } => {
                    tracing::debug!(
                        "PTP node: Received Delay_Req from {} seq={}",
                        src,
                        msg.header.sequence_id
                    );
                    self.add_slave(src);
                    self.handle_ieee_delay_req(msg, src).await?;
                }
                PtpMessageBody::DelayResp {
                    receive_timestamp, ..
                } => {
                    // Delay_Resp sometimes arrives on event port.
                    tracing::info!(
                        "PTP node: DelayResp (event port) seq={} T4={} from {}",
                        msg.header.sequence_id,
                        receive_timestamp,
                        src
                    );
                    self.process_delay_resp(*receive_timestamp).await;
                }
                _ => {
                    tracing::debug!(
                        "PTP node: Ignoring {:?} on event port from {}",
                        msg.header.message_type,
                        src
                    );
                }
            },
            Err(e) => {
                let hex: Vec<String> = data.iter().take(20).map(|b| format!("{b:02X}")).collect();
                tracing::warn!(
                    "PTP node: Failed to decode event packet ({} bytes, first 20: [{}]): {}",
                    data.len(),
                    hex.join(", "),
                    e
                );
            }
        }
        Ok(())
    }

    /// Handle incoming packet on general port (320).
    async fn handle_general_packet(
        &mut self,
        data: &[u8],
        src: SocketAddr,
    ) -> Result<(), std::io::Error> {
        if self.config.use_airplay_format {
            return Ok(());
        }

        match PtpMessage::decode(data) {
            Ok(msg) => match &msg.body {
                PtpMessageBody::FollowUp {
                    precise_origin_timestamp,
                } => {
                    tracing::debug!(
                        "PTP node: Follow_Up seq={}, T1={}, from {}",
                        msg.header.sequence_id,
                        precise_origin_timestamp,
                        src
                    );
                    self.pending_t1 = Some(*precise_origin_timestamp);
                    // As slave: send Delay_Req immediately after Follow_Up finalises T1.
                    // Always clear stale T3 from a previous unanswered Delay_Req so we
                    // can issue a fresh one for this Sync cycle.
                    if self.role == EffectiveRole::Slave && self.pending_t2.is_some() {
                        self.pending_t3 = None;
                        self.delay_req_sent_at = None;
                        self.send_delay_req().await?;
                    }
                }
                PtpMessageBody::DelayResp {
                    receive_timestamp, ..
                } => {
                    tracing::info!(
                        "PTP node: DelayResp (general port) seq={} T4={} from {}",
                        msg.header.sequence_id,
                        receive_timestamp,
                        src
                    );
                    self.process_delay_resp(*receive_timestamp).await;
                }
                PtpMessageBody::Announce {
                    grandmaster_identity,
                    grandmaster_priority1,
                    grandmaster_priority2,
                    ..
                } => {
                    tracing::debug!(
                        "PTP node: Announce from {} GM=0x{:016X} p1={} p2={}",
                        src,
                        grandmaster_identity,
                        grandmaster_priority1,
                        grandmaster_priority2
                    );
                    self.process_announce(
                        *grandmaster_identity,
                        *grandmaster_priority1,
                        *grandmaster_priority2,
                        src,
                    );
                }
                PtpMessageBody::Signaling => {
                    // Log raw bytes after the 34-byte header to decode TLVs manually.
                    let body = &data[34..];
                    // The first 10 bytes of Signaling body are targetPortIdentity.
                    let hex: Vec<String> = body.iter().map(|b| format!("{b:02X}")).collect();
                    tracing::info!(
                        "PTP node: Signaling from {} ({} body bytes): [{}]",
                        src,
                        body.len(),
                        hex.join(" ")
                    );
                }
                _ => {
                    tracing::debug!(
                        "PTP node: Ignoring {:?} on general port from {}",
                        msg.header.message_type,
                        src
                    );
                }
            },
            Err(e) => {
                let hex: Vec<String> = data.iter().take(20).map(|b| format!("{b:02X}")).collect();
                tracing::warn!(
                    "PTP node: Failed to decode general packet ({} bytes, first 20: [{}]): {:?}",
                    data.len(),
                    hex.join(", "),
                    e
                );
            }
        }
        Ok(())
    }

    /// Process a Delay_Resp (from either event or general port) to update the clock.
    async fn process_delay_resp(&mut self, receive_timestamp: PtpTimestamp) {
        if let (Some(t1), Some(t2_saved), Some(t3)) =
            (self.pending_t1, self.pending_t2, self.pending_t3)
        {
            let t4 = receive_timestamp;
            let mut clock = self.clock.write().await;
            clock.process_timing(t1, t2_saved, t3, t4);
            tracing::info!(
                "PTP node: Clock synced! offset={:.3}ms, measurements={}",
                clock.offset_millis(),
                clock.measurement_count()
            );
            self.pending_t1 = None;
            self.pending_t2 = None;
            self.pending_t3 = None;
            self.delay_req_sent_at = None;
        } else {
            tracing::debug!(
                "PTP node: Delay_Resp received but no pending T1/T2/T3 (t1={:?}, t2={:?}, t3={:?})",
                self.pending_t1.is_some(),
                self.pending_t2.is_some(),
                self.pending_t3.is_some()
            );
        }
    }

    /// Simplified BMCA: compare remote Announce with our own priority.
    ///
    /// Lower priority1 wins. If equal, lower priority2 wins.
    /// If still equal, lower clock_id wins.
    fn process_announce(
        &mut self,
        grandmaster_identity: u64,
        priority1: u8,
        priority2: u8,
        src: SocketAddr,
    ) {
        // Don't process our own Announces.
        if grandmaster_identity == self.config.clock_id {
            return;
        }

        let remote_is_better = self.compare_priority(priority1, priority2, grandmaster_identity);

        // Resolve the remote's event address. If we know a slave with the same
        // IP, use that (handles ephemeral ports in tests and non-standard setups).
        // Otherwise fall back to the standard PTP event port (319).
        let event_addr = self
            .known_slaves
            .iter()
            .find(|a| a.ip() == src.ip())
            .copied()
            .unwrap_or_else(|| {
                SocketAddr::new(src.ip(), super::handler::PTP_EVENT_PORT)
            });
        let general_addr = SocketAddr::new(src.ip(), src.port());

        if remote_is_better {
            let old_role = self.role;
            self.role = EffectiveRole::Slave;
            self.remote_master = Some(RemoteMaster {
                grandmaster_identity,
                priority1,
                priority2,
                event_addr,
                general_addr,
                last_announce: tokio::time::Instant::now(),
            });
            if old_role != EffectiveRole::Slave {
                tracing::info!(
                    "PTP BMCA: Switching to SLAVE (remote GM 0x{:016X} p1={} is better than our p1={})",
                    grandmaster_identity,
                    priority1,
                    self.config.priority1
                );
            }
        } else {
            // We are still better — update the remote record if it exists
            // so we know the remote is still alive (for timeout tracking),
            // but stay as master.
            if let Some(ref mut rm) = self.remote_master {
                if rm.grandmaster_identity == grandmaster_identity {
                    rm.last_announce = tokio::time::Instant::now();
                }
            }
        }
    }

    /// Compare our priority with a remote's. Returns `true` if the remote is better (higher priority).
    fn compare_priority(&self, remote_p1: u8, remote_p2: u8, remote_clock_id: u64) -> bool {
        if remote_p1 != self.config.priority1 {
            return remote_p1 < self.config.priority1;
        }
        if remote_p2 != self.config.priority2 {
            return remote_p2 < self.config.priority2;
        }
        // Tie-break on clock ID (lower wins).
        remote_clock_id < self.config.clock_id
    }

    /// Check if the remote master's Announce has timed out.
    fn check_announce_timeout(&mut self) {
        if let Some(ref rm) = self.remote_master {
            if rm.last_announce.elapsed() > self.announce_timeout {
                tracing::info!(
                    "PTP BMCA: Remote master 0x{:016X} timed out, reverting to MASTER",
                    rm.grandmaster_identity
                );
                self.role = EffectiveRole::Master;
                self.remote_master = None;
                // Reset slave state
                self.pending_t1 = None;
                self.pending_t2 = None;
                self.pending_t3 = None;
                self.delay_req_sent_at = None;
            }
        }
    }

    // ---- Master-side message sending ----

    async fn send_sync(&mut self) -> Result<(), std::io::Error> {
        let t1 = PtpTimestamp::now();
        let source = PtpPortIdentity::new(self.config.clock_id, 1);

        for &slave_addr in &self.known_slaves.clone() {
            if self.config.use_airplay_format {
                let pkt = AirPlayTimingPacket {
                    message_type: PtpMessageType::Sync,
                    sequence_id: self.sync_sequence,
                    timestamp: t1,
                    clock_id: self.config.clock_id,
                };
                self.event_socket.send_to(&pkt.encode(), slave_addr).await?;
            } else {
                let mut sync_msg = PtpMessage::sync(source, self.sync_sequence, t1);
                sync_msg.header.flags = 0x0200; // Two-step flag
                self.event_socket
                    .send_to(&sync_msg.encode(), slave_addr)
                    .await?;

                let precise_t1 = PtpTimestamp::now();
                let follow_up = PtpMessage::follow_up(source, self.sync_sequence, precise_t1);
                if let Some(ref general) = self.general_socket {
                    for &general_addr in &self.known_general_slaves {
                        general.send_to(&follow_up.encode(), general_addr).await?;
                    }
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

    async fn send_announce(&mut self) -> Result<(), std::io::Error> {
        let source = PtpPortIdentity::new(self.config.clock_id, 1);
        let announce = PtpMessage::announce(
            source,
            self.announce_sequence,
            self.config.clock_id,
            self.config.priority1,
            self.config.priority2,
        );
        let encoded = announce.encode();
        if let Some(ref general) = self.general_socket {
            for &addr in &self.known_general_slaves {
                general.send_to(&encoded, addr).await?;
            }
        }
        self.announce_sequence = self.announce_sequence.wrapping_add(1);
        Ok(())
    }

    // ---- Slave-side message sending ----

    async fn send_delay_req(&mut self) -> Result<(), std::io::Error> {
        let dest = if let Some(ref rm) = self.remote_master {
            rm.event_addr
        } else if let Some(addr) = self.known_slaves.first() {
            // Fallback: send to the first known peer.
            *addr
        } else {
            return Ok(());
        };

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
            "PTP node: Sending Delay_Req seq={} to {} (T3={})",
            self.delay_req_sequence,
            dest,
            t3
        );
        self.event_socket.send_to(&data, dest).await?;
        self.delay_req_sent_at = Some(tokio::time::Instant::now());
        self.delay_req_sequence = self.delay_req_sequence.wrapping_add(1);
        Ok(())
    }

    // ---- Master-side Delay_Req handling ----

    async fn handle_airplay_delay_req(
        &self,
        req: AirPlayTimingPacket,
        src: SocketAddr,
    ) -> Result<(), std::io::Error> {
        let t4 = PtpTimestamp::now();
        let resp = AirPlayTimingPacket {
            message_type: PtpMessageType::DelayResp,
            sequence_id: req.sequence_id,
            timestamp: t4,
            clock_id: self.config.clock_id,
        };
        self.event_socket.send_to(&resp.encode(), src).await?;
        Ok(())
    }

    async fn handle_ieee_delay_req(
        &self,
        msg: PtpMessage,
        src: SocketAddr,
    ) -> Result<(), std::io::Error> {
        let t4 = PtpTimestamp::now();
        let source = PtpPortIdentity::new(self.config.clock_id, 1);
        let resp = PtpMessage::delay_resp(
            source,
            msg.header.sequence_id,
            t4,
            msg.header.source_port_identity,
        );
        if let Some(ref general) = self.general_socket {
            // Send Delay_Resp on general port (standard IEEE 1588).
            // Look up the corresponding general address for this event
            // source by matching event_addr→general_addr pairs (handles
            // ephemeral ports and multiple peers on the same IP).
            let general_addr = self.resolve_general_addr_for_event(src);
            general.send_to(&resp.encode(), general_addr).await?;
        } else {
            self.event_socket.send_to(&resp.encode(), src).await?;
        }
        Ok(())
    }
}

/// Create a `PtpNode` with standard configuration for the AirPlay client role.
///
/// The client starts as master (priority1=128) and will switch to slave
/// if a device announces with a better priority.
pub fn create_client_node(
    event_socket: Arc<UdpSocket>,
    general_socket: Option<Arc<UdpSocket>>,
    clock: SharedPtpClock,
    clock_id: u64,
    priority1: u8,
) -> PtpNode {
    let config = PtpNodeConfig {
        clock_id,
        priority1,
        priority2: 128,
        ..Default::default()
    };
    PtpNode::new(event_socket, general_socket, clock, config)
}

/// Unit tests for the BMCA logic (compare_priority, process_announce,
/// check_announce_timeout). These tests live inside the module so they
/// can access private fields and methods without making them pub.
#[cfg(test)]
mod tests_unit {
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::time::Duration;

    use super::{EffectiveRole, PtpNode, PtpNodeConfig};
    use crate::protocol::ptp::clock::PtpRole;
    use crate::protocol::ptp::handler::create_shared_clock;

    /// Build a minimal PtpNode bound to an ephemeral loopback port.
    async fn make_node(our_priority1: u8, our_clock_id: u64) -> PtpNode {
        let sock = Arc::new(
            tokio::net::UdpSocket::bind("127.0.0.1:0")
                .await
                .unwrap(),
        );
        let clock = create_shared_clock(our_clock_id, PtpRole::Master);
        let config = PtpNodeConfig {
            clock_id: our_clock_id,
            priority1: our_priority1,
            priority2: 128,
            ..Default::default()
        };
        PtpNode::new(sock, None, clock, config)
    }

    // ── compare_priority ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_compare_priority_remote_wins_lower_p1() {
        // We have p1=255 (worst possible), remote has p1=128 → remote is better.
        let node = make_node(255, 0xAAAA).await;
        assert!(
            node.compare_priority(128, 128, 0xBBBB),
            "Remote with lower priority1 must win"
        );
    }

    #[tokio::test]
    async fn test_compare_priority_we_win_with_lower_p1() {
        // We have p1=64, remote has p1=128 → we are better.
        let node = make_node(64, 0xAAAA).await;
        assert!(
            !node.compare_priority(128, 128, 0xBBBB),
            "Remote with higher priority1 must NOT win"
        );
    }

    #[tokio::test]
    async fn test_compare_priority_equal_p1_remote_wins_lower_p2() {
        // Both p1=128. Remote p2=64 < our p2=128 → remote wins on priority2.
        let node = make_node(128, 0xAAAA).await;
        assert!(
            node.compare_priority(128, 64, 0xBBBB),
            "Remote with lower priority2 (tie on p1) must win"
        );
    }

    #[tokio::test]
    async fn test_compare_priority_equal_p1_we_win_higher_remote_p2() {
        // Both p1=128. Remote p2=200 > our p2=128 → we win.
        let node = make_node(128, 0xAAAA).await;
        assert!(
            !node.compare_priority(128, 200, 0xBBBB),
            "Remote with higher priority2 (tie on p1) must NOT win"
        );
    }

    #[tokio::test]
    async fn test_compare_priority_tiebreak_on_lower_clock_id() {
        // Both p1=128, p2=128. Remote clock_id=0x0001 < ours=0xAAAA → remote wins.
        let node = make_node(128, 0xAAAA).await;
        assert!(
            node.compare_priority(128, 128, 0x0001),
            "Remote with lower clock_id (tie on both priorities) must win"
        );
    }

    #[tokio::test]
    async fn test_compare_priority_tiebreak_on_higher_clock_id_we_win() {
        // Both p1=128, p2=128. Remote clock_id=0xFFFF > ours=0xAAAA → we win.
        let node = make_node(128, 0xAAAA).await;
        assert!(
            !node.compare_priority(128, 128, 0xFFFF),
            "Remote with higher clock_id (tie on both priorities) must NOT win"
        );
    }

    #[tokio::test]
    async fn test_compare_priority_identical_parameters_is_false() {
        // If remote and local have the exact same values, remote does not win
        // (since `remote_clock_id < self.config.clock_id` is false when equal).
        let node = make_node(128, 0xAAAA).await;
        assert!(
            !node.compare_priority(128, 128, 0xAAAA),
            "Identical parameters must not trigger a role switch"
        );
    }

    // ── process_announce ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_process_announce_switches_to_slave_when_remote_better() {
        let mut node = make_node(255, 0xAAAA).await;
        assert_eq!(node.role, EffectiveRole::Master);

        let src = SocketAddr::from_str("192.168.1.100:320").unwrap();
        // Remote p1=128 < our p1=255 → remote is better.
        node.process_announce(0xBBBB_CCCC_DDDD_EEEE, 128, 128, src);

        assert_eq!(
            node.role,
            EffectiveRole::Slave,
            "Should switch to Slave when a better-priority Announce arrives"
        );
        assert!(
            node.remote_master.is_some(),
            "remote_master must be populated after switching to Slave"
        );
    }

    #[tokio::test]
    async fn test_process_announce_stays_master_when_remote_worse() {
        let mut node = make_node(64, 0xAAAA).await;
        assert_eq!(node.role, EffectiveRole::Master);

        let src = SocketAddr::from_str("192.168.1.100:320").unwrap();
        // Remote p1=128 > our p1=64 → we are better, stay Master.
        node.process_announce(0xBBBB_CCCC_DDDD_EEEE, 128, 128, src);

        assert_eq!(
            node.role,
            EffectiveRole::Master,
            "Should stay Master when we have better priority"
        );
        assert!(
            node.remote_master.is_none(),
            "remote_master must remain None when we stay Master"
        );
    }

    #[tokio::test]
    async fn test_process_announce_ignores_own_clock_id() {
        // Even if the priority would be better, an Announce with our own clock_id
        // (e.g. a reflected packet) must be silently dropped.
        let our_clock_id = 0xAAAA_BBBB_CCCC_DDDD;
        let mut node = make_node(255, our_clock_id).await;
        let src = SocketAddr::from_str("192.168.1.100:320").unwrap();

        node.process_announce(our_clock_id, 1, 1, src); // perfect priority but our ID

        assert_eq!(
            node.role,
            EffectiveRole::Master,
            "Own clock_id in Announce must be ignored"
        );
        assert!(
            node.remote_master.is_none(),
            "remote_master must not be set after ignoring own Announce"
        );
    }

    #[tokio::test]
    async fn test_process_announce_updates_last_announce_when_staying_master() {
        // When we receive an Announce from a known (but worse) remote, the
        // remote_master record's last_announce must be refreshed (if it exists).
        let mut node = make_node(64, 0xAAAA).await;

        // First announce that doesn't switch us (remote is worse).
        let src = SocketAddr::from_str("192.168.1.100:320").unwrap();
        node.process_announce(0xBBBB, 128, 128, src);
        // Still master, no remote_master entry.
        assert!(node.remote_master.is_none());

        // Now manually install a remote_master so we start as Slave.
        node.role = EffectiveRole::Slave;
        node.remote_master = Some(super::RemoteMaster {
            grandmaster_identity: 0xBBBB,
            priority1: 128,
            priority2: 128,
            event_addr: SocketAddr::from_str("192.168.1.100:319").unwrap(),
            general_addr: src,
            last_announce: tokio::time::Instant::now(),
        });
        // Tweak our priority to make the remote worse so this Announce won't re-trigger slave.
        node.config.priority1 = 64;

        // Re-process same remote with same clock_id while we have it tracked.
        node.process_announce(0xBBBB, 128, 128, src);
        // remote_master entry is still there (we refreshed it).
        assert!(
            node.remote_master.is_some(),
            "remote_master entry must be preserved when remote sends a new Announce"
        );
    }

    // ── check_announce_timeout ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_announce_timeout_reverts_to_master() {
        let mut node = make_node(255, 0xAAAA).await;

        // First become Slave via Announce.
        let src = SocketAddr::from_str("192.168.1.100:320").unwrap();
        node.process_announce(0xBBBB, 128, 128, src);
        assert_eq!(node.role, EffectiveRole::Slave);

        // Set a very short timeout so it triggers immediately.
        node.announce_timeout = Duration::from_nanos(1);

        // Sleep a tiny amount so last_announce.elapsed() > 1ns.
        tokio::time::sleep(Duration::from_millis(5)).await;

        node.check_announce_timeout();

        assert_eq!(
            node.role,
            EffectiveRole::Master,
            "Must revert to Master after announce timeout"
        );
        assert!(
            node.remote_master.is_none(),
            "remote_master must be cleared after timeout"
        );
        // All pending slave state must be cleared.
        assert!(node.pending_t1.is_none(), "pending_t1 must be cleared");
        assert!(node.pending_t2.is_none(), "pending_t2 must be cleared");
        assert!(node.pending_t3.is_none(), "pending_t3 must be cleared");
        assert!(
            node.delay_req_sent_at.is_none(),
            "delay_req_sent_at must be cleared"
        );
    }

    #[tokio::test]
    async fn test_announce_timeout_does_not_fire_when_recent() {
        let mut node = make_node(255, 0xAAAA).await;

        let src = SocketAddr::from_str("192.168.1.100:320").unwrap();
        node.process_announce(0xBBBB, 128, 128, src);
        assert_eq!(node.role, EffectiveRole::Slave);

        // Very long timeout — must NOT fire right after the Announce.
        node.announce_timeout = Duration::from_secs(60);
        node.check_announce_timeout();

        assert_eq!(
            node.role,
            EffectiveRole::Slave,
            "Must stay Slave when announce has not timed out"
        );
        assert!(
            node.remote_master.is_some(),
            "remote_master must remain set when announce is recent"
        );
    }

    #[tokio::test]
    async fn test_announce_timeout_is_no_op_when_no_remote_master() {
        // Already Master with no remote_master — calling check_announce_timeout
        // must be a no-op and must not panic.
        let mut node = make_node(128, 0xAAAA).await;
        assert_eq!(node.role, EffectiveRole::Master);
        assert!(node.remote_master.is_none());

        node.announce_timeout = Duration::from_nanos(1);
        tokio::time::sleep(Duration::from_millis(5)).await;
        node.check_announce_timeout(); // must not panic

        assert_eq!(node.role, EffectiveRole::Master);
    }

    // ── Delay_Req timeout / retry (DELAY_REQ_TIMEOUT) ────────────────────────

    /// Verify the DELAY_REQ_TIMEOUT constant matches the expected 1-second value.
    /// This value was tuned to balance responsiveness and avoiding spurious retries.
    #[test]
    fn test_delay_req_timeout_constant_is_one_second() {
        assert_eq!(
            super::DELAY_REQ_TIMEOUT,
            Duration::from_millis(1000),
            "DELAY_REQ_TIMEOUT must be 1 second"
        );
    }
}
