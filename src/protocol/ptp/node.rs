//! Unified PTP node that participates in both master and slave roles.
//!
//! A `PtpNode` can simultaneously:
//! - Send `Sync`/`Follow_Up` and respond to `Delay_Req` (master behaviour)
//! - Process incoming `Sync`/`Follow_Up`, send `Delay_Req`, and process `Delay_Resp` (slave
//!   behaviour)
//! - Evaluate Announce messages and switch roles via a simplified BMCA
//!
//! This is needed because `AirPlay` 2 devices (e.g. `HomePod`) may act as
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
    /// Interval between `Delay_Req` messages when acting as slave.
    pub delay_req_interval: Duration,
    /// Interval between Announce messages.
    pub announce_interval: Duration,
    /// Maximum receive buffer size.
    pub recv_buf_size: usize,
    /// Use `AirPlay` compact packet format instead of IEEE 1588.
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
#[allow(
    dead_code,
    reason = "Fields retained for diagnostics and future BMCA extensions"
)]
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

/// Unified PTP node supporting bidirectional synchronization.
///
/// Runs a single event loop that handles both master and slave message
/// flows on the same sockets. Uses a simplified BMCA to determine
/// whether this node should act as master or slave.
pub struct PtpNode {
    /// Event socket (port 319 or `AirPlay` timing port).
    event_socket: Arc<UdpSocket>,
    /// General socket (port 320), optional if using `AirPlay` format.
    general_socket: Option<Arc<UdpSocket>>,
    /// Shared clock state.
    clock: SharedPtpClock,
    /// Configuration.
    config: PtpNodeConfig,
    /// Current effective role.
    role: EffectiveRole,
    /// Next Sync sequence ID (master).
    sync_sequence: u16,
    /// Next `Delay_Req` sequence ID (slave).
    delay_req_sequence: u16,
    /// Next Announce sequence ID.
    announce_sequence: u16,
    /// Known slave addresses for Sync broadcasts (master role).
    known_slaves: Vec<SocketAddr>,
    /// Known slave general addresses for `Follow_Up` (master role).
    known_general_slaves: Vec<SocketAddr>,
    /// Pending Sync T1 (slave role).
    pending_t1: Option<PtpTimestamp>,
    /// T2 corresponding to pending T1 (slave role).
    pending_t2: Option<PtpTimestamp>,
    /// Pending `Delay_Req` T3 (slave role).
    pending_t3: Option<PtpTimestamp>,
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

    /// Add a known slave general address for `Follow_Up` messages.
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
    #[allow(
        clippy::too_many_lines,
        reason = "Event loop handles multiple socket selectors and timer events in a unified \
                  manner"
    )]
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
                            self.handle_general_packet(&general_buf[..len], src).await;
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

                // Periodic Delay_Req: send when we have a pending Sync (T1/T2)
                // but haven't yet sent Delay_Req (no pending T3).
                // In BMCA mode this only fires when role==Slave; in AirPlay
                // compact format (no BMCA) we always respond to received Syncs.
                _ = delay_req_timer.tick() => {
                    let should_send = self.pending_t1.is_some()
                        && self.pending_t3.is_none()
                        && (self.role == EffectiveRole::Slave || self.config.use_airplay_format);
                    if should_send {
                        self.send_delay_req().await?;
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
                        "PTP node: Received Sync from {} seq={}, two_step={}, T1={}",
                        src,
                        msg.header.sequence_id,
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
                    tracing::debug!(
                        "PTP node: DelayResp (event port) seq={}, T4={}",
                        msg.header.sequence_id,
                        receive_timestamp
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
    async fn handle_general_packet(&mut self, data: &[u8], src: SocketAddr) {
        if self.config.use_airplay_format {
            return;
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
                }
                PtpMessageBody::DelayResp {
                    receive_timestamp, ..
                } => {
                    tracing::debug!(
                        "PTP node: DelayResp (general port) seq={}, T4={}, from {}",
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
                    tracing::debug!("PTP node: Signaling from {}", src);
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
    }

    /// Process a `Delay_Resp` (from either event or general port) to update the clock.
    async fn process_delay_resp(&mut self, receive_timestamp: PtpTimestamp) {
        if let (Some(t1), Some(t2_saved), Some(t3)) =
            (self.pending_t1, self.pending_t2, self.pending_t3)
        {
            let t4 = receive_timestamp;
            let mut clock = self.clock.write().await;
            clock.process_timing(t1, t2_saved, t3, t4);
            tracing::info!(
                "PTP node: Clock synced (offset={:.3}ms, measurements={})",
                clock.offset_millis(),
                clock.measurement_count()
            );
            self.pending_t1 = None;
            self.pending_t2 = None;
            self.pending_t3 = None;
        }
    }

    /// Simplified BMCA: compare remote Announce with our own priority.
    ///
    /// Lower priority1 wins. If equal, lower priority2 wins.
    /// If still equal, lower `clock_id` wins.
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
            .unwrap_or_else(|| SocketAddr::new(src.ip(), super::handler::PTP_EVENT_PORT));
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
                    "PTP BMCA: Switching to SLAVE (remote GM 0x{:016X} p1={} is better than our \
                     p1={})",
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

    /// Compare our priority with a remote's. Returns `true` if the remote is better (higher
    /// priority).
    fn compare_priority(&self, remote_p1: u8, remote_p2: u8, remote_clock_id: u64) -> bool {
        if remote_p1 != self.config.priority1 {
            return remote_p1 < self.config.priority1;
        }
        if remote_p2 != self.config.priority2 {
            return remote_p2 < self.config.priority2;
        }
        // Tie-break on `clock_id` (lower wins).
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

        tracing::debug!(
            "PTP node: Sending Delay_Req seq={} to {}",
            self.delay_req_sequence,
            dest
        );
        self.event_socket.send_to(&data, dest).await?;
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

/// Create a `PtpNode` with standard configuration for the `AirPlay` client role.
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
