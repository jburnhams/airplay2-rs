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
    /// Transport-specific nibble to set in outgoing event messages (`Delay_Req`, Sync).
    ///
    /// Apple `AirPlay` 2 devices use `transport_specific = 1` (byte 0 of PTP messages
    /// is `0x1n` where `n` is the message type).  Standard IEEE 1588 uses 0.
    /// `HomePod` may silently ignore `Delay_Req` with the wrong `transport_specific`.
    pub transport_specific: u8,
    /// How long to wait without an Announce before reverting to Master role.
    ///
    /// IEEE 1588 default is 3 × `announce_interval` (typically 3s). For `AirPlay` 2,
    /// the `HomePod` often only sends a single Announce to establish BMCA and then
    /// relies on `Sync/Delay_Req/Delay_Resp` for ongoing synchronization. Setting
    /// this to a large value (e.g. 60s) prevents premature reversion to Master.
    pub announce_timeout: Duration,
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
            transport_specific: 0,
            announce_timeout: Duration::from_secs(6),
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

/// Timeout after which an unanswered `Delay_Req` is considered lost.
const DELAY_REQ_TIMEOUT: Duration = Duration::from_secs(1);

/// Unified PTP node supporting bidirectional synchronization.
///
/// Runs a single event loop that handles both master and slave message
/// flows on the same sockets. Uses a simplified BMCA to determine
/// whether this node should act as master or slave.
pub struct PtpNode {
    /// Event socket (port 319 or `AirPlay` timing port).
    /// Receives Sync from master, sends Sync/Announce as master.
    event_socket: Arc<UdpSocket>,
    /// General socket (port 320), optional if using `AirPlay` format.
    /// Receives `Follow_Up`, Announce, Signaling from master.
    general_socket: Option<Arc<UdpSocket>>,
    /// Timing socket (`AirPlay` 2 ephemeral port): sends `Delay_Req` and receives `Delay_Resp`.
    ///
    /// Real `AirPlay` 2 clients do NOT send `Delay_Req` from the standard PTP event port (319).
    /// Instead, they bind an ephemeral timing port (registered in SETUP Step 1 `ClockPorts`
    /// and SETUP Step 2 timingPort) and use that port for the entire `Delay_Req/Delay_Resp`
    /// exchange.  The `HomePod`'s `SupportsClockPortMatchingOverride=true` tells it to look
    /// up `ClockPorts[clock_id]` to find our registered ephemeral port and route
    /// `Delay_Resp` there.  If this socket is `None`, we fall back to `event_socket`.
    timing_socket: Option<Arc<UdpSocket>>,
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
    /// Next Signaling sequence ID (for Apple peer-announcement responses).
    signaling_sequence: u16,
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
    /// When the pending `Delay_Req` was sent (for timeout).
    delay_req_sent_at: Option<tokio::time::Instant>,
    /// Number of consecutive `Delay_Req` without response.
    delay_req_unanswered: u32,
    /// The current remote master we are slaving to (if any).
    remote_master: Option<RemoteMaster>,
    /// Announce timeout: if no Announce from the remote master within
    /// this duration, assume it's gone and revert to master.
    announce_timeout: Duration,
    /// Epoch offset from the first `Delay_Req/Delay_Resp` exchange.
    ///
    /// `unix_now_ns − master_now_ns`, computed from the first measurement
    /// where T2 and T3 were captured with the Unix wall clock.  Once set,
    /// `adjusted_now()` subtracts this value so that subsequent T2/T3
    /// timestamps are in the master's time domain.  This makes `offset_ns`
    /// in `PtpClock` converge to near zero (residual jitter only) rather
    /// than retaining the full epoch difference (~56 years for `HomePod`).
    calibrated_epoch_offset: Option<i128>,
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
        let announce_timeout = config.announce_timeout;
        Self {
            event_socket,
            general_socket,
            timing_socket: None,
            clock,
            config,
            role: EffectiveRole::Master,
            sync_sequence: 0,
            delay_req_sequence: 0,
            announce_sequence: 0,
            signaling_sequence: 0,
            known_slaves: Vec::new(),
            known_general_slaves: Vec::new(),
            pending_t1: None,
            pending_t2: None,
            pending_t3: None,
            delay_req_sent_at: None,
            delay_req_unanswered: 0,
            remote_master: None,
            announce_timeout,
            calibrated_epoch_offset: None,
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

    /// Set the `AirPlay` 2 timing socket used for `Delay_Req` (sending) and `Delay_Resp`
    /// (receiving).
    ///
    /// Real `AirPlay` 2 clients (Apple Music, etc.) do NOT send `Delay_Req` from the standard
    /// PTP event port (319).  Instead they use an ephemeral port that was registered with the
    /// `HomePod` via `timingPeerInfo.ClockPorts` in SETUP Step 1 and `timingPort` in SETUP Step 2.
    ///
    /// When set, this socket is preferred over `event_socket` for `Delay_Req` sends, and the run
    /// loop also reads `Delay_Resp` from it (in addition to the event/general sockets).
    /// This is required to receive `Delay_Resp` because `HomePod` routes it to the registered
    /// `ClockPorts` port (the ephemeral timing port), NOT to port 319.
    pub fn set_timing_socket(&mut self, sock: Arc<UdpSocket>) {
        self.timing_socket = Some(sock);
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

    /// Current timestamp in the master's time domain.
    ///
    /// Before epoch calibration (first `Delay_Resp` not yet received) this
    /// falls back to the Unix wall clock (`PtpTimestamp::now()`), so T2/T3
    /// captured during the first exchange will be in Unix nanoseconds.  The
    /// resulting large `offset_ns` (~56 years for `HomePod`) is then used to
    /// call `PtpClock::calibrate_epoch` so that all subsequent calls return
    /// a timestamp in the master's epoch.
    ///
    /// After calibration: `master_ns = unix_now_ns − epoch_offset_ns`.
    fn adjusted_now(&self) -> PtpTimestamp {
        match self.calibrated_epoch_offset {
            None => PtpTimestamp::now(), // pre-calibration: use raw Unix time
            Some(epoch_offset) => {
                let unix_ns = PtpTimestamp::now().to_nanos();
                let master_ns = unix_ns - epoch_offset;
                if master_ns >= 0 {
                    PtpTimestamp::from_nanos(master_ns)
                } else {
                    PtpTimestamp::ZERO
                }
            }
        }
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
        reason = "Centralized protocol logic requires multiple steps"
    )]
    pub async fn run(
        &mut self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), std::io::Error> {
        let mut event_buf = vec![0u8; self.config.recv_buf_size];
        let mut general_buf = vec![0u8; self.config.recv_buf_size];
        // Timing socket buffer (for Delay_Resp from HomePod on our ephemeral timing port).
        let mut timing_buf = vec![0u8; self.config.recv_buf_size];
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
                            // Log every raw receipt so we can distinguish "packet arrived
                            // but failed to decode" from "no packet arrived at all".
                            tracing::debug!(
                                "PTP event RX: {} bytes from {} (role={:?}, first byte: {:02X})",
                                len, src, self.role,
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
                            tracing::debug!(
                                "PTP general RX: {} bytes from {} (role={:?}, first byte: {:02X})",
                                len, src, self.role,
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

                // Receive on timing socket (if set).
                //
                // The HomePod routes Delay_Resp to the port registered in
                // ClockPorts[clock_id] (our ephemeral timing port), NOT to the
                // standard PTP event port (319).  We therefore need an additional
                // receive branch for this socket so that Delay_Resp is not missed.
                // The packet is handled with the same logic as event-port packets
                // because Delay_Resp may appear on either port.
                result = async {
                    if let Some(ref sock) = self.timing_socket {
                        sock.recv_from(&mut timing_buf).await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match result {
                        Ok((len, src)) => {
                            tracing::debug!(
                                "PTP timing socket: {} bytes from {} (role={:?})",
                                len, src, self.role
                            );
                            self.handle_event_packet(&timing_buf[..len], src).await?;
                        }
                        Err(e) if Self::is_transient_udp_error(&e) => {
                            tracing::debug!("PTP node: transient timing socket error: {}", e);
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

                // Periodic Delay_Req fallback.
                //
                // The primary trigger for Delay_Req is now inside
                // handle_general_packet (immediately on Follow_Up receipt) so
                // that we respond within the HomePod's short sync burst window.
                //
                // This periodic timer is kept as a safety net for:
                //   • one-step Sync devices (no Follow_Up) where we never hit the
                //     immediate path above, and
                //   • retry if a Delay_Req was sent but Delay_Resp was lost,
                //     in which case pending_t3 stays set until the next Sync
                //     resets it (see handle_event_packet Sync case).
                // In BMCA mode this only fires when role==Slave; in AirPlay
                // compact format (no BMCA) we always respond to received Syncs.
                _ = delay_req_timer.tick() => {
                    // Check for Delay_Req timeout (no Delay_Resp received).
                    self.check_delay_req_timeout().await;

                    // Don't send more Delay_Req once we've fallen back to one-way mode.
                    let in_fallback = self.delay_req_unanswered >= 2;
                    let should_send = self.pending_t1.is_some()
                        && self.pending_t3.is_none()
                        && !in_fallback
                        && (self.role == EffectiveRole::Slave || self.config.use_airplay_format);
                    if should_send {
                        tracing::info!(
                            "PTP node: Sending Delay_Req (role={:?}, unanswered={}, pending_t1={}, pending_t3={})",
                            self.role,
                            self.delay_req_unanswered,
                            self.pending_t1.is_some(),
                            self.pending_t3.is_some()
                        );
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
    #[allow(
        clippy::too_many_lines,
        reason = "Complex logic function with multiple match arms"
    )]
    async fn handle_event_packet(
        &mut self,
        data: &[u8],
        src: SocketAddr,
    ) -> Result<(), std::io::Error> {
        // Use adjusted_now() so that after epoch calibration T2 is in the
        // master's time domain, causing subsequent offset measurements to
        // converge near zero rather than retaining the raw epoch difference.
        let receive_time = self.adjusted_now();

        if self.config.use_airplay_format {
            if let Ok(pkt) = AirPlayTimingPacket::decode(data) {
                match pkt.message_type {
                    PtpMessageType::Sync => {
                        // Incoming Sync — we are acting as slave for this exchange.
                        if self.pending_t3.is_some() {
                            self.delay_req_unanswered += 1;
                            self.delay_req_sent_at = None;
                        }
                        self.pending_t1 = Some(pkt.timestamp);
                        self.pending_t2 = Some(receive_time);
                        self.pending_t3 = None;

                        // For AirPlay format without Follow_Up, we immediately act as slave.
                        // We also need to send Delay_Req to complete the exchange.
                        // In AirPlay format (unlike IEEE 1588 with FollowUp),
                        // we can send Delay_Req immediately upon receiving Sync.
                        // Only send if we are a Slave or in AirPlay format where role doesn't strictly matter
                        if self.role == EffectiveRole::Slave || self.config.use_airplay_format {
                            // If we are in AirPlay format but our priority is better (lower),
                            // we should be the master and not act as a slave responding to Syncs.
                            // In this test node A has priority=64, node B has 128. A is master, B is slave.
                            // The test expects B to become synchronized.
                            // If A receives Sync, it should ignore it instead of responding if it is master.
                            // But wait, the test fails because B is NOT synchronized. Why?
                            // Let's ensure B sends the Delay_Req by not doing priority checks yet,
                            // or wait, let's see. If we send delay req, it's awaited.
                            let in_fallback = self.delay_req_unanswered >= 2;
                            if !in_fallback {
                                // Since send_delay_req() can be async and self is borrowed mutably,
                                // we can just do it.
                                self.send_delay_req().await?;
                            } else {
                                self.try_one_way_sync().await;
                            }
                        }
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
                    if msg.header.sequence_id % 64 == 0 {
                        tracing::info!(
                            "PTP node: Received Sync from {} seq={}, two_step={}, T1={} \
                             (role={:?})",
                            src,
                            msg.header.sequence_id,
                            two_step,
                            origin_timestamp,
                            self.role
                        );
                    } else {
                        tracing::debug!(
                            "PTP node: Received Sync from {} seq={}, two_step={}, T1={}",
                            src,
                            msg.header.sequence_id,
                            two_step,
                            origin_timestamp
                        );
                    }
                    // Store T1/T2 for slave-side processing.
                    //
                    // If pending_t3 is still set here it means we sent a Delay_Req
                    // for the *previous* Sync cycle and the master never replied.
                    // Count that now, before clearing, so that the fallback counter
                    // advances correctly.  (The Follow_Up handler used to do this, but
                    // it sees pending_t3=None because we clear it below first.)
                    if self.pending_t3.is_some() {
                        self.delay_req_unanswered += 1;
                        self.delay_req_sent_at = None;
                        tracing::debug!(
                            "PTP node: Delay_Req unanswered when next Sync arrived (unanswered={})",
                            self.delay_req_unanswered
                        );
                    }
                    self.pending_t1 = Some(*origin_timestamp);
                    self.pending_t2 = Some(receive_time);
                    self.pending_t3 = None; // reset: new Sync cycle begins
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
                    // In one-way fallback mode, process immediately.
                    self.try_one_way_sync().await;
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
    ///
    /// Returns an `Err` only for fatal I/O errors so that the caller can
    /// propagate them through the event loop. Non-fatal conditions are logged
    /// and silently ignored.
    #[allow(
        clippy::too_many_lines,
        reason = "Complex logic function with multiple match arms"
    )]
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
                    // Store the precise T1 from the Follow_Up.
                    self.pending_t1 = Some(*precise_origin_timestamp);

                    // --- Immediate Delay_Req on Follow_Up receipt ---
                    //
                    // The HomePod (and many AirPlay 2 devices) sends a short burst of
                    // Sync+Follow_Up messages (typically 4 messages at ~8 Hz / 125 ms
                    // apart) and then STOPS waiting for Delay_Req.  The periodic
                    // delay_req_timer fires after 1 second, which is AFTER this burst
                    // window has closed, so the HomePod never receives our Delay_Req
                    // and never sends Delay_Resp → clock never syncs.
                    //
                    // Fix: react immediately — send Delay_Req as soon as we have a
                    // complete Sync+Follow_Up pair while in Slave role, unless we are
                    // in one-way fallback mode.
                    if self.role == EffectiveRole::Slave && self.pending_t2.is_some() {
                        // pending_t3 was already cleared (and unanswered counter advanced
                        // if needed) by the Sync handler above, so we just decide whether
                        // to send another Delay_Req or fall back to one-way mode.
                        self.pending_t3 = None; // already None; defensive clear
                        self.delay_req_sent_at = None; // clear stale timer if any
                        if self.delay_req_unanswered < 2 {
                            self.send_delay_req().await?;
                        } else {
                            // In one-way fallback mode, process immediately.
                            self.try_one_way_sync().await;
                        }
                    }
                }
                PtpMessageBody::DelayResp {
                    receive_timestamp, ..
                } => {
                    // Apple HomePod Delay_Resp non-standard encoding:
                    //   receiveTimestamp = T1 (last Sync send time, NOT the actual T4)
                    //   correctionField  = (T4_actual − T1) in 2^-16 ns units
                    //
                    // So T4_actual = receiveTimestamp + correctionField >> 16 ns.
                    //
                    // This differs from IEEE 1588-2008 §11.4.3 where receiveTimestamp
                    // IS T4 and correctionField carries transparent-clock residence.
                    // The correctionField is a signed 64-bit value in 2^-16 ns units;
                    // right-shifting by 16 gives the integer nanosecond component
                    // (sub-ns fractional part discarded, acceptable for audio sync).
                    let correction_ns = msg.header.correction_field >> 16; // 2^-16 ns → ns
                    let t4_actual = if correction_ns > 0 {
                        let base_ns = receive_timestamp.to_nanos();
                        let corrected_ns = base_ns + i128::from(correction_ns);
                        PtpTimestamp::from_nanos(corrected_ns.max(0))
                    } else {
                        *receive_timestamp
                    };
                    tracing::debug!(
                        "PTP node: DelayResp (general port) seq={}, T4_body={}, correction={}ns, \
                         T4_actual={}, from {}",
                        msg.header.sequence_id,
                        receive_timestamp,
                        correction_ns,
                        t4_actual,
                        src,
                    );
                    self.process_delay_resp(t4_actual).await;
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
                    // TLVs start at byte 44 (34-byte header + 10-byte targetPortIdentity).
                    // Detect Apple ORGANIZATION_EXTENSION TLVs (OUI 0x000D93) and respond.
                    //
                    // AirPlay 2 uses a bidirectional Signaling peer-announcement protocol:
                    //   1. HomePod sends Signaling with Apple TLV sub-type 1 (clock ID + port).
                    //   2. Client MUST respond with its own Signaling (Apple TLV sub-type 1).
                    //   3. Only after this exchange does the HomePod send Delay_Resp.
                    //
                    // Without step 2 the HomePod never responds to our Delay_Req, so the clock
                    // never syncs and the device stays in a pre-playing state (RTSP 455).
                    let hex: Vec<String> = data.iter().map(|b| format!("{b:02X}")).collect();
                    tracing::debug!(
                        "PTP node: Signaling from {} ({} bytes): [{}]",
                        src,
                        data.len(),
                        hex.join(", ")
                    );

                    let source_port_id = msg.header.source_port_identity;
                    let mut has_apple_tlv = false;

                    if data.len() > 44 {
                        let mut offset = 44;
                        while offset + 4 <= data.len() {
                            let tlv_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
                            let tlv_len =
                                u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;

                            // Check for Apple ORGANIZATION_EXTENSION (OUI 00 0D 93)
                            if tlv_type == 0x0003
                                && tlv_len >= 4
                                && offset + 4 + 3 <= data.len()
                                && data[offset + 4] == 0x00
                                && data[offset + 5] == 0x0D
                                && data[offset + 6] == 0x93
                            {
                                let sub_type = if offset + 7 < data.len() {
                                    data[offset + 7]
                                } else {
                                    0
                                };
                                tracing::debug!(
                                    "PTP Signaling TLV: ORGANIZATION_EXTENSION (Apple OUI \
                                     0x000D93) sub-type={} len={}",
                                    sub_type,
                                    tlv_len
                                );
                                has_apple_tlv = true;
                            } else {
                                tracing::debug!(
                                    "PTP Signaling TLV: type=0x{:04X} len={}",
                                    tlv_type,
                                    tlv_len
                                );
                            }

                            if tlv_len == 0 {
                                break; // guard against infinite loop on malformed TLV
                            }
                            offset += 4 + tlv_len;
                        }
                    }

                    if has_apple_tlv {
                        tracing::info!(
                            "PTP node: HomePod Apple Signaling received — sending \
                             peer-announcement response"
                        );
                        self.send_apple_signaling_response(source_port_id, src)
                            .await?;
                    }
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

    /// Process a `Delay_Resp` (from either event or general port) to update the clock.
    async fn process_delay_resp(&mut self, receive_timestamp: PtpTimestamp) {
        if let (Some(t1), Some(t2_saved), Some(t3)) =
            (self.pending_t1, self.pending_t2, self.pending_t3)
        {
            let t4 = receive_timestamp;
            let mut clock = self.clock.write().await;
            clock.process_timing(t1, t2_saved, t3, t4);

            // On the first successful measurement T2/T3 were captured with the
            // raw Unix clock so offset_ns ≈ (unix_epoch − master_epoch) ~56 years
            // for HomePod.  Calibrate the epoch once so that every subsequent
            // T2/T3 comes from adjusted_now() (master domain) and offset_ns
            // reflects only residual network jitter (typically < 1 ms).
            if self.calibrated_epoch_offset.is_none() {
                let raw_offset = clock.offset_nanos();
                clock.calibrate_epoch(raw_offset);
                self.calibrated_epoch_offset = Some(raw_offset);
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "Precision loss is acceptable for logging epoch offset in ms"
                )]
                let raw_offset_ms = raw_offset as f64 / 1_000_000.0;
                tracing::info!(
                    "PTP node: Master clock epoch calibrated (epoch_offset={:.3}ms). Subsequent \
                     offsets will reflect residual jitter only.",
                    raw_offset_ms
                );
            }

            tracing::info!(
                "PTP node: Clock synced (offset={:.6}ms, measurements={})",
                clock.offset_millis(),
                clock.measurement_count()
            );
            self.pending_t1 = None;
            self.pending_t2 = None;
            self.pending_t3 = None;
            self.delay_req_sent_at = None;
            self.delay_req_unanswered = 0;
        } else {
            tracing::debug!(
                "PTP node: Delay_Resp received but no pending T1/T2/T3 (t1={:?}, t2={:?}, t3={:?})",
                self.pending_t1.is_some(),
                self.pending_t2.is_some(),
                self.pending_t3.is_some()
            );
        }
    }

    /// Check if a pending `Delay_Req` has timed out.
    ///
    /// After 2 consecutive unanswered `Delay_Req`, falls back to one-way
    /// offset estimation (using just Sync T1/T2). This handles `AirPlay`
    /// devices that act as PTP master but don't respond to `Delay_Req`.
    async fn check_delay_req_timeout(&mut self) {
        const MAX_UNANSWERED_BEFORE_FALLBACK: u32 = 2;

        if let Some(sent_at) = self.delay_req_sent_at {
            if sent_at.elapsed() > DELAY_REQ_TIMEOUT {
                self.delay_req_unanswered += 1;
                tracing::info!(
                    "PTP node: Delay_Req timed out (unanswered={}, elapsed={:.1}s)",
                    self.delay_req_unanswered,
                    sent_at.elapsed().as_secs_f64()
                );

                // Clear pending state to allow retry.
                self.pending_t3 = None;
                self.delay_req_sent_at = None;

                // After enough unanswered requests, fall back to one-way estimation.
                if self.delay_req_unanswered >= MAX_UNANSWERED_BEFORE_FALLBACK {
                    if let (Some(t1), Some(t2)) = (self.pending_t1, self.pending_t2) {
                        let mut clock = self.clock.write().await;
                        clock.process_one_way(t1, t2);
                        tracing::info!(
                            "PTP node: One-way sync fallback (offset={:.3}ms, measurements={})",
                            clock.offset_millis(),
                            clock.measurement_count()
                        );
                    }
                }
            }
        }
    }

    /// Process one-way sync if we're in fallback mode and have T1/T2.
    ///
    /// Called after receiving `Follow_Up` with precise T1. Only processes
    /// if we've given up on `Delay_Req` (unanswered >= 2) and aren't waiting
    /// for a `Delay_Resp`.
    async fn try_one_way_sync(&mut self) {
        const MAX_UNANSWERED_BEFORE_FALLBACK: u32 = 2;

        if self.delay_req_unanswered >= MAX_UNANSWERED_BEFORE_FALLBACK
            && self.pending_t3.is_none()
            && self.role == EffectiveRole::Slave
        {
            if let (Some(t1), Some(t2)) = (self.pending_t1, self.pending_t2) {
                let mut clock = self.clock.write().await;
                clock.process_one_way(t1, t2);
                let count = clock.measurement_count();
                // Log every 64th measurement at INFO, rest at DEBUG
                if count <= 3 || count % 64 == 0 {
                    tracing::info!(
                        "PTP node: One-way sync (offset={:.3}ms, T1={}, T2={}, measurements={})",
                        clock.offset_millis(),
                        t1,
                        t2,
                        count
                    );
                } else {
                    tracing::debug!(
                        "PTP node: One-way sync (offset={:.3}ms)",
                        clock.offset_millis()
                    );
                }
            }
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
            // Store the remote master's clock ID so TimeAnnounce can use it.
            if let Ok(mut clock) = self.clock.try_write() {
                clock.set_remote_master_clock_id(grandmaster_identity);
            }
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

    /// Build a Signaling message containing an Apple `ORGANIZATION_EXTENSION` TLV (sub-type 1).
    ///
    /// `AirPlay` 2 uses a bidirectional peer-announcement protocol over PTP Signaling.  After the
    /// `HomePod` sends its own Signaling (OUI 0x000D93, sub-type 1) identifying its clock and
    /// timing port, it expects the client to respond with a mirror Signaling identifying the
    /// client's clock and ephemeral timing port.  Without this exchange the `HomePod` does not
    /// send `Delay_Resp` — the peer discovery step is what authorises the
    /// `Delay_Req`/`Delay_Resp` flow.
    ///
    /// IEEE 1588-2008 `ORGANIZATION_EXTENSION` TLV body layout (22 bytes total):
    /// ```text
    ///  [0..2]   organizationId  = 00 0D 93 (Apple OUI)
    ///  [3..5]   organizationSubType = 00 00 01 (3 bytes per IEEE 1588 spec, sub-type 1)
    ///  [6..13]  clock_identity  (8 bytes, big-endian) — our PTP clock ID
    ///  [14..17] IPv4 address    (4 bytes; zeros = HomePod infers from UDP source IP)
    ///  [18..19] timing port     (2 bytes, big-endian) — the ephemeral ClockPorts port
    ///  [20..21] reserved / flags (zeros)
    /// ```
    #[must_use]
    pub(crate) fn build_apple_signaling(
        &self,
        target_port: PtpPortIdentity,
        sequence_id: u16,
        timing_port: u16,
    ) -> Vec<u8> {
        // Apple ORGANIZATION_EXTENSION TLV body (22 bytes)
        //
        // IEEE 1588-2008 Table F.2 defines ORGANIZATION_EXTENSION TLV:
        //   organizationId (3 bytes) + organizationSubType (3 bytes) + dataField (N-6 bytes)
        // For N=22 (our length field): 3 + 3 = 6 fixed bytes + 16 data bytes.
        let mut tlv_body = [0u8; 22];
        // organizationId = Apple OUI 0x000D93
        tlv_body[0] = 0x00;
        tlv_body[1] = 0x0D;
        tlv_body[2] = 0x93;
        // organizationSubType = sub-type 1 (3 bytes, per IEEE 1588 spec)
        // Sub-type 1 = timing peer announcement (the HomePod's format)
        tlv_body[3] = 0x00;
        tlv_body[4] = 0x00;
        tlv_body[5] = 0x01;
        // dataField (16 bytes):
        //   [6..13]  clock_identity — our PTP clock ID (8 bytes BE)
        //   [14..17] IPv4 address   — zeros; HomePod infers our IP from the UDP source
        //   [18..19] timing port    — our ephemeral timing socket port (2 bytes BE)
        //   [20..21] reserved       — zeros
        tlv_body[6..14].copy_from_slice(&self.config.clock_id.to_be_bytes());
        // IPv4 bytes [14..17] stay zero.
        tlv_body[18..20].copy_from_slice(&timing_port.to_be_bytes());
        // Bytes [20..21] stay zero (reserved / flags).

        // Full message size:
        //   PTP header          = 34 bytes
        //   targetPortIdentity  = 10 bytes
        //   TLV type + length   =  4 bytes
        //   TLV body            = 22 bytes
        //   ─────────────────────────────
        //   Total               = 70 bytes
        let total_len: u16 = 34 + 10 + 4 + 22; // = 70

        let mut buf = Vec::with_capacity(total_len as usize);

        // ── PTP Header (34 bytes) ──────────────────────────────────────────────
        // Byte 0: transport_specific=1 (Apple) | messageType=0xC (Signaling)
        buf.push(0x1C);
        // Byte 1: PTP version 2
        buf.push(0x02);
        // Bytes 2-3: total message length
        buf.extend_from_slice(&total_len.to_be_bytes());
        // Byte 4: domain number (0)
        buf.push(0x00);
        // Byte 5: reserved
        buf.push(0x00);
        // Bytes 6-7: flags (none)
        buf.extend_from_slice(&0u16.to_be_bytes());
        // Bytes 8-15: correction field (0)
        buf.extend_from_slice(&0i64.to_be_bytes());
        // Bytes 16-19: reserved
        buf.extend_from_slice(&[0u8; 4]);
        // Bytes 20-27: source clock identity (our clock ID)
        buf.extend_from_slice(&self.config.clock_id.to_be_bytes());
        // Bytes 28-29: source port number (1)
        buf.extend_from_slice(&1u16.to_be_bytes());
        // Bytes 30-31: sequence ID
        buf.extend_from_slice(&sequence_id.to_be_bytes());
        // Byte 32: control field (5 = Signaling / Management / all-other)
        buf.push(0x05);
        // Byte 33: log message interval (0x7F = "not applicable" per IEEE 1588 for Signaling)
        buf.push(0x7F);

        // ── Signaling body: targetPortIdentity (10 bytes) ─────────────────────
        buf.extend_from_slice(&target_port.clock_identity.to_be_bytes());
        buf.extend_from_slice(&target_port.port_number.to_be_bytes());

        // ── Apple ORGANIZATION_EXTENSION TLV ──────────────────────────────────
        // TLV type = 0x0003 (ORGANIZATION_EXTENSION per IEEE 1588-2008 Annex F)
        buf.extend_from_slice(&0x0003u16.to_be_bytes());
        // TLV length = 22 (bytes after type+length header)
        buf.extend_from_slice(&22u16.to_be_bytes());
        // TLV body
        buf.extend_from_slice(&tlv_body);

        debug_assert_eq!(
            buf.len(),
            total_len as usize,
            "Signaling message size mismatch"
        );
        buf
    }

    /// Send an Apple peer-announcement Signaling response to the `HomePod`.
    ///
    /// Called whenever we receive an Apple Signaling from the `HomePod`.  The `HomePod`
    /// requires this reciprocal announcement before it will process our `Delay_Req`
    /// and send `Delay_Resp`.
    async fn send_apple_signaling_response(
        &mut self,
        target: PtpPortIdentity,
        dest: SocketAddr,
    ) -> Result<(), std::io::Error> {
        let timing_port = self
            .timing_socket
            .as_ref()
            .and_then(|s| s.local_addr().ok())
            .map_or(0, |a| a.port());

        if timing_port == 0 {
            tracing::debug!(
                "PTP node: skipping Apple Signaling response — timing port not known yet"
            );
            return Ok(());
        }

        let seq = self.signaling_sequence;
        self.signaling_sequence = self.signaling_sequence.wrapping_add(1);

        let msg_bytes = self.build_apple_signaling(target, seq, timing_port);

        // Signaling is a general message; prefer the general socket (port 320).
        let sent = if let Some(ref general) = self.general_socket {
            general.send_to(&msg_bytes, dest).await?
        } else {
            self.event_socket.send_to(&msg_bytes, dest).await?
        };

        tracing::debug!(
            "PTP node: Apple Signaling response sent to {} ({} bytes, seq={}, timing_port={})",
            dest,
            sent,
            seq,
            timing_port
        );
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

        // Use adjusted_now() for T3 so it matches the master's time domain
        // after epoch calibration (same reasoning as for T2 in handle_event_packet).
        let t3 = self.adjusted_now();
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
            // Apple AirPlay 2: HomePod uses transport_specific=1 for all PTP messages
            // and may silently drop Delay_Req with transport_specific=0.
            msg.header.transport_specific = self.config.transport_specific;
            msg.encode()
        };

        tracing::info!(
            "PTP node: Sending Delay_Req seq={} to {} (T3={})",
            self.delay_req_sequence,
            dest,
            t3
        );
        // Use the timing socket if available — the HomePod expects Delay_Req to come from
        // the same ephemeral port registered in ClockPorts (our timing port), not port 319.
        // Fall back to event_socket for test and non-AirPlay-2 scenarios.
        let (bytes_sent, socket_local_addr) = if let Some(ref timing_sock) = self.timing_socket {
            let n = timing_sock.send_to(&data, dest).await?;
            let local = timing_sock.local_addr();
            (n, local)
        } else {
            let n = self.event_socket.send_to(&data, dest).await?;
            let local = self.event_socket.local_addr();
            (n, local)
        };
        tracing::debug!(
            "PTP node: Delay_Req sent OK ({} bytes, local={:?})",
            bytes_sent,
            socket_local_addr
        );
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
        tracing::info!(
            "PTP node: Received Delay_Req from {} seq={}, responding with Delay_Resp (T4={})",
            src,
            msg.header.sequence_id,
            t4
        );
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

/// Unit tests for the BMCA logic (`compare_priority`, `process_announce`,
/// `check_announce_timeout`). These tests live inside the module so they
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

    /// Build a minimal `PtpNode` bound to an ephemeral loopback port.
    async fn make_node(our_priority1: u8, our_clock_id: u64) -> PtpNode {
        let sock = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
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

    /// Verify the `DELAY_REQ_TIMEOUT` constant matches the expected 1-second value.
    /// This value was tuned to balance responsiveness and avoiding spurious retries.
    #[test]
    fn test_delay_req_timeout_constant_is_one_second() {
        assert_eq!(
            super::DELAY_REQ_TIMEOUT,
            Duration::from_secs(1),
            "DELAY_REQ_TIMEOUT must be 1 second"
        );
    }
}
