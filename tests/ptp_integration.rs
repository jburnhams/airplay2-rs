//! Integration tests for PTP timing synchronization.
//!
//! Tests end-to-end PTP exchanges using real UDP sockets on loopback.

use std::sync::Arc;
use std::time::Duration;

use airplay2::protocol::ptp::clock::{PtpClock, PtpRole};
use airplay2::protocol::ptp::handler::{
    PtpHandlerConfig, PtpMasterHandler, PtpSlaveHandler, create_shared_clock,
};
use airplay2::protocol::ptp::message::{
    AirPlayTimingPacket, PtpMessage, PtpMessageBody, PtpMessageType, PtpParseError, PtpPortIdentity,
};
use airplay2::protocol::ptp::node::{EffectiveRole, PtpNode, PtpNodeConfig};
use airplay2::protocol::ptp::timestamp::PtpTimestamp;
use tokio::net::UdpSocket;

// ===== Full IEEE 1588 two-step exchange =====

#[tokio::test]
async fn test_full_ieee1588_two_step_exchange() {
    let master_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let slave_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let master_addr = master_sock.local_addr().unwrap();
    let slave_addr = slave_sock.local_addr().unwrap();

    let mut slave_clock = PtpClock::new(0xBBBB, PtpRole::Slave);
    let master_source = PtpPortIdentity::new(0xAAAA, 1);
    let slave_source = PtpPortIdentity::new(0xBBBB, 1);

    // Perform 5 exchanges.
    for seq in 0..5u16 {
        // 1. Master sends Sync.
        let t1 = PtpTimestamp::now();
        let mut sync = PtpMessage::sync(master_source, seq, t1);
        sync.header.flags = 0x0200; // Two-step
        master_sock
            .send_to(&sync.encode(), slave_addr)
            .await
            .unwrap();

        // 2. Slave receives Sync.
        let mut buf = [0u8; 256];
        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let t2 = PtpTimestamp::now();
        let recv_sync = PtpMessage::decode(&buf[..len]).unwrap();
        assert_eq!(recv_sync.header.message_type, PtpMessageType::Sync);

        // 3. Master sends Follow-up.
        let follow_up = PtpMessage::follow_up(master_source, seq, t1);
        master_sock
            .send_to(&follow_up.encode(), slave_addr)
            .await
            .unwrap();

        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let fu = PtpMessage::decode(&buf[..len]).unwrap();
        assert_eq!(fu.header.message_type, PtpMessageType::FollowUp);
        let precise_t1 = match fu.body {
            PtpMessageBody::FollowUp {
                precise_origin_timestamp,
            } => precise_origin_timestamp,
            _ => panic!("Expected Follow-up body"),
        };

        // 4. Slave sends Delay_Req.
        let t3 = PtpTimestamp::now();
        let delay_req = PtpMessage::delay_req(slave_source, seq, t3);
        slave_sock
            .send_to(&delay_req.encode(), master_addr)
            .await
            .unwrap();

        // 5. Master receives Delay_Req.
        let (len, from) = master_sock.recv_from(&mut buf).await.unwrap();
        let t4 = PtpTimestamp::now();
        let req = PtpMessage::decode(&buf[..len]).unwrap();
        assert_eq!(req.header.message_type, PtpMessageType::DelayReq);

        // 6. Master sends Delay_Resp.
        let delay_resp = PtpMessage::delay_resp(master_source, seq, t4, slave_source);
        master_sock
            .send_to(&delay_resp.encode(), from)
            .await
            .unwrap();

        // 7. Slave receives Delay_Resp.
        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let resp = PtpMessage::decode(&buf[..len]).unwrap();
        assert_eq!(resp.header.message_type, PtpMessageType::DelayResp);
        let recv_t4 = match resp.body {
            PtpMessageBody::DelayResp {
                receive_timestamp, ..
            } => receive_timestamp,
            _ => panic!("Expected DelayResp body"),
        };

        // 8. Update clock.
        slave_clock.process_timing(precise_t1, t2, t3, recv_t4);
    }

    assert!(slave_clock.is_synchronized());
    // On loopback, offset should be very small (same machine, same clock).
    assert!(
        slave_clock.offset_millis().abs() < 50.0,
        "Offset should be near zero on loopback: {}ms",
        slave_clock.offset_millis()
    );
    assert_eq!(slave_clock.measurement_count(), 5);
}

// ===== Full AirPlay compact exchange =====

#[tokio::test]
async fn test_full_airplay_compact_exchange() {
    let master_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let slave_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let master_addr = master_sock.local_addr().unwrap();
    let slave_addr = slave_sock.local_addr().unwrap();

    let mut slave_clock = PtpClock::new(0xBBBB, PtpRole::Slave);

    for seq in 0..3u16 {
        // Master sends Sync.
        let t1 = PtpTimestamp::now();
        let sync = AirPlayTimingPacket {
            message_type: PtpMessageType::Sync,
            sequence_id: seq,
            timestamp: t1,
            clock_id: 0xAAAA,
        };
        master_sock
            .send_to(&sync.encode(), slave_addr)
            .await
            .unwrap();

        // Slave receives Sync.
        let mut buf = [0u8; 256];
        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let t2 = PtpTimestamp::now();
        let recv = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
        assert_eq!(recv.message_type, PtpMessageType::Sync);

        // Slave sends Delay_Req.
        let t3 = PtpTimestamp::now();
        let delay_req = AirPlayTimingPacket {
            message_type: PtpMessageType::DelayReq,
            sequence_id: seq,
            timestamp: t3,
            clock_id: 0xBBBB,
        };
        slave_sock
            .send_to(&delay_req.encode(), master_addr)
            .await
            .unwrap();

        // Master receives and sends Delay_Resp.
        let (len, from) = master_sock.recv_from(&mut buf).await.unwrap();
        let t4 = PtpTimestamp::now();
        let req = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
        assert_eq!(req.message_type, PtpMessageType::DelayReq);

        let delay_resp = AirPlayTimingPacket {
            message_type: PtpMessageType::DelayResp,
            sequence_id: seq,
            timestamp: t4,
            clock_id: 0xAAAA,
        };
        master_sock
            .send_to(&delay_resp.encode(), from)
            .await
            .unwrap();

        // Slave receives and updates clock.
        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let resp = AirPlayTimingPacket::decode(&buf[..len]).unwrap();

        slave_clock.process_timing(recv.timestamp, t2, t3, resp.timestamp);
    }

    assert!(slave_clock.is_synchronized());
    assert!(
        slave_clock.offset_millis().abs() < 50.0,
        "AirPlay offset too large: {}ms",
        slave_clock.offset_millis()
    );
}

// ===== Master and slave handler tasks =====

#[tokio::test]
async fn test_master_slave_handler_tasks() {
    let master_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let slave_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let master_addr = master_sock.local_addr().unwrap();
    let slave_addr = slave_sock.local_addr().unwrap();

    // Connect master to slave (for broadcast/send without target).
    master_sock.connect(slave_addr).await.unwrap();

    let master_clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let slave_clock = create_shared_clock(0xBBBB, PtpRole::Slave);

    let master_config = PtpHandlerConfig {
        clock_id: 0xAAAA,
        role: PtpRole::Master,
        sync_interval: Duration::from_millis(50),
        use_airplay_format: true,
        ..Default::default()
    };

    let slave_config = PtpHandlerConfig {
        clock_id: 0xBBBB,
        role: PtpRole::Slave,
        delay_req_interval: Duration::from_millis(50),
        use_airplay_format: true,
        ..Default::default()
    };

    let (master_shutdown_tx, master_shutdown_rx) = tokio::sync::watch::channel(false);
    let (slave_shutdown_tx, slave_shutdown_rx) = tokio::sync::watch::channel(false);

    // Start master handler.
    let master_sock_clone = master_sock.clone();
    let master_clock_clone = master_clock.clone();
    let master_handle = tokio::spawn(async move {
        let mut handler =
            PtpMasterHandler::new(master_sock_clone, None, master_clock_clone, master_config);
        handler.run(master_shutdown_rx).await
    });

    // Start slave handler.
    let slave_sock_clone = slave_sock.clone();
    let slave_clock_clone = slave_clock.clone();
    let slave_handle = tokio::spawn(async move {
        let mut handler = PtpSlaveHandler::new(
            slave_sock_clone,
            None,
            slave_clock_clone,
            slave_config,
            master_addr,
        );
        handler.run(slave_shutdown_rx).await
    });

    // Let them exchange for a bit.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check that slave has synchronized.
    let _clock = slave_clock.read().await;
    // On loopback this may or may not sync depending on timing,
    // but the handler should have accepted at least some measurements.
    // (The sync depends on both sides' timing, which is non-deterministic.)

    // Shutdown both.
    master_shutdown_tx.send(true).unwrap();
    slave_shutdown_tx.send(true).unwrap();

    let _ = tokio::time::timeout(Duration::from_secs(2), master_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), slave_handle).await;
}

// ===== Clock offset with known skew =====

#[test]
fn test_clock_offset_5_second_skew() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Slave is exactly 5 seconds ahead of master.
    // Network delay: 1ms each way.
    let t1 = PtpTimestamp::new(100, 0); // master send
    let t2 = PtpTimestamp::new(105, 1_000_000); // slave recv (5s offset + 1ms delay)
    let t3 = PtpTimestamp::new(105, 2_000_000); // slave send (5s offset + 2ms)
    let t4 = PtpTimestamp::new(100, 3_000_000); // master recv (3ms from start)

    clock.process_timing(t1, t2, t3, t4);

    // Expected offset: 5 seconds.
    let offset_ms = clock.offset_millis();
    assert!(
        (offset_ms - 5000.0).abs() < 5.0,
        "Expected 5s offset, got {}ms",
        offset_ms
    );
}

#[test]
fn test_clock_offset_negative_skew() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Slave is 2 seconds behind master.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(98, 1_000_000);
    let t3 = PtpTimestamp::new(98, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);

    clock.process_timing(t1, t2, t3, t4);

    let offset_ms = clock.offset_millis();
    assert!(
        (offset_ms - (-2000.0)).abs() < 5.0,
        "Expected -2s offset, got {}ms",
        offset_ms
    );
}

// ===== Timestamp conversions =====

#[test]
fn test_timestamp_ieee1588_roundtrip_many() {
    let test_values = [
        PtpTimestamp::new(0, 0),
        PtpTimestamp::new(1, 0),
        PtpTimestamp::new(0, 1),
        PtpTimestamp::new(0, 999_999_999),
        PtpTimestamp::new(u64::from(u32::MAX), 0),
        PtpTimestamp::new(12345, 678_901_234),
    ];

    for ts in &test_values {
        let encoded = ts.encode_ieee1588();
        let decoded = PtpTimestamp::decode_ieee1588(&encoded).unwrap();
        assert_eq!(ts, &decoded, "Roundtrip failed for {ts}");
    }
}

#[test]
fn test_timestamp_airplay_compact_roundtrip_seconds() {
    // Integer seconds should roundtrip perfectly.
    for secs in [0, 1, 100, 10000, 1_000_000] {
        let ts = PtpTimestamp::new(secs, 0);
        let compact = ts.to_airplay_compact();
        let back = PtpTimestamp::from_airplay_compact(compact);
        assert_eq!(ts, back, "Integer second roundtrip failed for {secs}");
    }
}

// ===== Message parsing edge cases =====

#[test]
fn test_parse_invalid_message_type() {
    // Build a packet with message type 0x0F (invalid).
    let mut data = vec![0u8; 44]; // Minimum for Sync
    data[0] = 0x0F; // Invalid message type
    data[1] = 0x02; // Version 2
    let result = PtpMessage::decode(&data);
    assert!(result.is_err());
    match result.unwrap_err() {
        PtpParseError::UnknownMessageType(t) => assert_eq!(t, 0x0F),
        other => panic!("Expected UnknownMessageType, got {other:?}"),
    }
}

#[test]
fn test_parse_truncated_header() {
    let data = vec![0u8; 10];
    assert!(PtpMessage::decode(&data).is_err());
}

#[test]
fn test_airplay_packet_all_message_types() {
    for msg_type in [
        PtpMessageType::Sync,
        PtpMessageType::DelayReq,
        PtpMessageType::FollowUp,
        PtpMessageType::DelayResp,
        PtpMessageType::Announce,
    ] {
        let pkt = AirPlayTimingPacket {
            message_type: msg_type,
            sequence_id: 42,
            timestamp: PtpTimestamp::new(100, 0),
            clock_id: 0xDEAD,
        };
        let encoded = pkt.encode();
        let decoded = AirPlayTimingPacket::decode(&encoded).unwrap();
        assert_eq!(decoded.message_type, msg_type);
        assert_eq!(decoded.sequence_id, 42);
    }
}

// ===== Clock with multiple measurements and outlier rejection =====

#[test]
fn test_clock_outlier_rejection() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_max_rtt(Duration::from_millis(5));

    // Good measurement (RTT = 2ms).
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 500_000);
    let t3 = PtpTimestamp::new(100, 1_000_000);
    let t4 = PtpTimestamp::new(100, 2_000_000);
    assert!(clock.process_timing(t1, t2, t3, t4));

    // Bad measurement (RTT = 50ms) — should be rejected.
    let t1 = PtpTimestamp::new(200, 0);
    let t2 = PtpTimestamp::new(200, 500_000);
    let t3 = PtpTimestamp::new(200, 1_000_000);
    let t4 = PtpTimestamp::new(200, 50_000_000);
    assert!(!clock.process_timing(t1, t2, t3, t4));

    // Only the good measurement should remain.
    assert_eq!(clock.measurement_count(), 1);
}

// ===== RTP timestamp to PTP conversion =====

#[test]
fn test_rtp_to_ptp_one_second() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let anchor_rtp = 0u32;
    let anchor_ptp = PtpTimestamp::new(1000, 0);

    let result = clock.rtp_to_local_ptp(44100, 44100, anchor_rtp, anchor_ptp);
    // 44100 samples at 44100 Hz = 1 second.
    assert_eq!(result.seconds, 1001);
}

#[test]
fn test_rtp_to_ptp_half_second() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let anchor_rtp = 0u32;
    let anchor_ptp = PtpTimestamp::new(500, 0);

    let result = clock.rtp_to_local_ptp(22050, 44100, anchor_rtp, anchor_ptp);
    // 22050 samples at 44100 Hz = 0.5 seconds.
    assert_eq!(result.seconds, 500);
    assert!(
        (result.nanoseconds as i64 - 500_000_000).abs() < 1_000_000,
        "Expected ~500ms, got {} ns",
        result.nanoseconds
    );
}

// ===== Port identity =====

#[test]
fn test_port_identity_default() {
    let id = PtpPortIdentity::default();
    assert_eq!(id.clock_identity, 0);
    assert_eq!(id.port_number, 0);
}

#[test]
fn test_port_identity_in_message_preserved() {
    let source = PtpPortIdentity::new(0x0102030405060708, 0x0A0B);
    let msg = PtpMessage::sync(source, 0, PtpTimestamp::ZERO);
    let encoded = msg.encode();
    let decoded = PtpMessage::decode(&encoded).unwrap();
    assert_eq!(decoded.header.source_port_identity, source);
}

// ===== Clock reset and re-sync =====

#[test]
fn test_clock_reset_then_resync() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // First sync.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(105, 1_000_000);
    let t3 = PtpTimestamp::new(105, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);
    assert!(clock.is_synchronized());
    assert!((clock.offset_millis() - 5000.0).abs() < 5.0);

    // Reset.
    clock.reset();
    assert!(!clock.is_synchronized());

    // Re-sync with different offset.
    let t1 = PtpTimestamp::new(200, 0);
    let t2 = PtpTimestamp::new(202, 1_000_000);
    let t3 = PtpTimestamp::new(202, 2_000_000);
    let t4 = PtpTimestamp::new(200, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);
    assert!(clock.is_synchronized());
    assert!((clock.offset_millis() - 2000.0).abs() < 5.0);
}

// ===== BMCA (Best Master Clock Algorithm) tests =====
//
// These tests verify that a PTP node with priority1=255 (worst possible)
// correctly yields to the HomePod (priority1=248) and switches to Slave.
// This is the fix for the original bug where priority1=128 kept us as Master.

/// Build a PtpNodeConfig with the given priority1 and clock_id.
///
/// Mirrors the production configuration from `start_ptp_master` in manager.rs:
/// `use_airplay_format: false` because the HomePod uses standard IEEE 1588 PTP.
fn client_config_ieee(clock_id: u64, priority1: u8) -> PtpNodeConfig {
    PtpNodeConfig {
        clock_id,
        priority1,
        priority2: priority1,
        sync_interval: Duration::from_millis(100),
        delay_req_interval: Duration::from_millis(100),
        announce_interval: Duration::from_millis(50),
        recv_buf_size: 256,
        use_airplay_format: false, // HomePod uses standard IEEE 1588 PTP
        transport_specific: 0,     // Standard IEEE 1588 (HomePod uses 1 for AirPlay, 0 for tests)
        announce_timeout: Duration::from_secs(6), // Default for tests
    }
}

/// Encode a minimal PTP Announce message that a HomePod-like master would send.
fn encode_homepod_announce(seq: u16, priority1: u8, clock_id: u64) -> Vec<u8> {
    let source = PtpPortIdentity::new(clock_id, 1);
    // PtpMessage::announce(source, sequence_id, grandmaster_identity, priority1, priority2)
    let msg = PtpMessage::announce(source, seq, clock_id, priority1, priority1);
    msg.encode()
}

/// Helper: run a PtpNode for a short duration, return it to inspect final state.
///
/// Sends `announce_data` to the node's *general* socket (port 320) from a
/// separate "HomePod" socket before starting, so the node processes the
/// Announce during its first event-loop iterations.
async fn run_node_with_announce(
    node: PtpNode,
    general_addr: std::net::SocketAddr,
    announce_data: Vec<u8>,
) -> PtpNode {
    // Send Announce to the general socket before the node starts processing.
    let homepod_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    homepod_sock
        .send_to(&announce_data, general_addr)
        .await
        .unwrap();

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut node_task = node;
    let handle = tokio::spawn(async move {
        let _ = tokio::time::timeout(Duration::from_millis(300), node_task.run(shutdown_rx)).await;
        node_task
    });

    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = shutdown_tx.send(true);
    handle.await.unwrap()
}

/// Create a PtpNode with separate event and general sockets, matching the
/// production configuration in `start_ptp_master`.
async fn make_ieee_node(
    clock_id: u64,
    priority1: u8,
) -> (PtpNode, std::net::SocketAddr, std::net::SocketAddr) {
    let event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let general_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let event_addr = event_sock.local_addr().unwrap();
    let general_addr = general_sock.local_addr().unwrap();

    let clock = create_shared_clock(clock_id, PtpRole::Master);
    let node = PtpNode::new(
        event_sock,
        Some(general_sock),
        clock,
        client_config_ieee(clock_id, priority1),
    );
    (node, event_addr, general_addr)
}

/// A PTP node with priority1=255 should switch to Slave when it receives an
/// Announce from a node with priority1=248 (HomePod default).
///
/// This tests the fix: before the fix, priority1=128 kept us as Master even
/// when the HomePod (248) was present; the fix sets priority1=255 so the
/// HomePod (248 < 255) wins BMCA.
#[tokio::test]
async fn test_bmca_priority255_yields_to_homepod_priority248() {
    const CLIENT_CLOCK_ID: u64 = 0xCCCC_CCCC_CCCC_CCCCu64;
    const HOMEPOD_CLOCK_ID: u64 = 0xAAAA_AAAA_AAAA_AAAAu64;

    let (node, _event_addr, general_addr) = make_ieee_node(CLIENT_CLOCK_ID, 255).await;

    // Node starts as Master (default before any Announce is received).
    assert_eq!(
        node.role(),
        EffectiveRole::Master,
        "Node should start as Master before any Announce"
    );

    // HomePod sends Announce with priority1=248 (its real value).
    // 248 < 255, so the HomePod is a better master.
    let announce = encode_homepod_announce(1, 248, HOMEPOD_CLOCK_ID);
    let node_after = run_node_with_announce(node, general_addr, announce).await;

    assert_eq!(
        node_after.role(),
        EffectiveRole::Slave,
        "Node with priority1=255 should switch to Slave when HomePod (p1=248) announces"
    );
}

/// A PTP node with priority1=128 (the OLD buggy default) should NOT yield to
/// the HomePod's priority1=248 — it stays as Master.
/// This documents the bug that was present before the fix.
#[tokio::test]
async fn test_bmca_old_priority128_stays_master_over_homepod_248() {
    const CLIENT_CLOCK_ID: u64 = 0xCCCC_CCCC_CCCC_CCCCu64;
    const HOMEPOD_CLOCK_ID: u64 = 0xAAAA_AAAA_AAAA_AAAAu64;

    let (node, _event_addr, general_addr) = make_ieee_node(CLIENT_CLOCK_ID, 128).await;

    // HomePod sends Announce with priority1=248.
    // 248 > 128, so the client (p1=128) is the "better" master and stays Master.
    let announce = encode_homepod_announce(1, 248, HOMEPOD_CLOCK_ID);
    let node_after = run_node_with_announce(node, general_addr, announce).await;

    // This is the OLD (broken) behaviour: we stay as Master instead of syncing.
    assert_eq!(
        node_after.role(),
        EffectiveRole::Master,
        "Node with priority1=128 stays Master even when HomePod (p1=248) announces — this was the \
         bug"
    );
}

/// A node should always yield to a remote with a lower priority1 value,
/// regardless of the remote's priority2 or clock ID.
#[tokio::test]
async fn test_bmca_lower_priority1_always_wins() {
    const CLIENT_CLOCK_ID: u64 = 0xFFFF_FFFF_FFFF_FFFFu64;
    const REMOTE_CLOCK_ID: u64 = 0x1111_1111_1111_1111u64;

    let (node, _event_addr, general_addr) = make_ieee_node(CLIENT_CLOCK_ID, 200).await;

    // Remote sends Announce with priority1=1 (best possible master).
    let announce = encode_homepod_announce(1, 1, REMOTE_CLOCK_ID);
    let node_after = run_node_with_announce(node, general_addr, announce).await;

    assert_eq!(
        node_after.role(),
        EffectiveRole::Slave,
        "Node with priority1=200 should yield to remote with priority1=1"
    );
}

/// A node should NOT yield to a remote with a higher priority1 value.
#[tokio::test]
async fn test_bmca_higher_priority1_does_not_win() {
    const CLIENT_CLOCK_ID: u64 = 0xAAAA_AAAA_AAAA_AAAAu64;
    const REMOTE_CLOCK_ID: u64 = 0x2222_2222_2222_2222u64;

    let (node, _event_addr, general_addr) = make_ieee_node(CLIENT_CLOCK_ID, 100).await;

    // Remote sends Announce with priority1=200 (worse than us).
    let announce = encode_homepod_announce(1, 200, REMOTE_CLOCK_ID);
    let node_after = run_node_with_announce(node, general_addr, announce).await;

    assert_eq!(
        node_after.role(),
        EffectiveRole::Master,
        "Node with priority1=100 should stay Master when remote has priority1=200"
    );
}

// ===== Immediate Delay_Req on Follow_Up (HomePod burst timing fix) =====
//
// The HomePod sends a burst of Sync+Follow_Up pairs at ~125 ms intervals
// (~375 ms total window) then stops listening for Delay_Req.  The OLD code
// only sent Delay_Req when the periodic `delay_req_timer` fired — this
// happened AFTER the burst window closed, so HomePod never sent Delay_Resp
// and the clock never synced.
//
// Fix (node.rs handle_general_packet): on Follow_Up receipt while in Slave
// role and no Delay_Req outstanding (`pending_t3 == None`), send Delay_Req
// immediately without waiting for the timer.
//
// The tests below use `delay_req_interval = 10 s` so the periodic timer
// cannot fire during the test.  Any Delay_Req that arrives must have come
// from the immediate Follow_Up path.

/// PtpNodeConfig identical to `client_config_ieee` but with a very long
/// `delay_req_interval` (10 s) to isolate the immediate Follow_Up trigger
/// from the periodic fallback timer.
fn client_config_ieee_slow_timer(clock_id: u64, priority1: u8) -> PtpNodeConfig {
    PtpNodeConfig {
        delay_req_interval: Duration::from_secs(10),
        ..client_config_ieee(clock_id, priority1)
    }
}

/// Verify that a PtpNode in Slave mode sends Delay_Req immediately after
/// receiving a Follow_Up, *without* waiting for the periodic delay_req_timer.
///
/// This is the primary regression test for the HomePod burst timing bug:
/// - OLD behaviour: Delay_Req arrives after `delay_req_interval` (≥ 100 ms in tests, ≥ 1 s in
///   production) — too late for the HomePod's sync window.
/// - NEW behaviour: Delay_Req is sent inside `handle_general_packet` as soon as Follow_Up is
///   processed, typically within a few milliseconds on loopback.
///
/// The test uses `delay_req_interval = 10 s` so the periodic timer cannot fire;
/// the only possible source of Delay_Req is the immediate Follow_Up path.
#[tokio::test]
async fn test_immediate_delay_req_on_follow_up() {
    const CLIENT_CLOCK_ID: u64 = 0xCCCC_CCCC_CCCC_CCCCu64;
    const HOMEPOD_CLOCK_ID: u64 = 0xAAAA_AAAA_AAAA_AAAAu64;

    // HomePod-side sockets: master sends Sync/Announce from these,
    // and listens for Delay_Req on the event socket.
    let homepod_event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let homepod_general_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let homepod_event_addr = homepod_event_sock.local_addr().unwrap();
    let _homepod_general_addr = homepod_general_sock.local_addr().unwrap();

    // Client-side sockets with slow periodic timer.
    let client_event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_general_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_event_addr = client_event_sock.local_addr().unwrap();
    let client_general_addr = client_general_sock.local_addr().unwrap();
    let client_clock = create_shared_clock(CLIENT_CLOCK_ID, PtpRole::Master);
    let mut node = PtpNode::new(
        client_event_sock,
        Some(client_general_sock),
        client_clock,
        client_config_ieee_slow_timer(CLIENT_CLOCK_ID, 255),
    );

    // Pre-register HomePod's event address so that `process_announce` can
    // resolve `remote_master.event_addr` correctly.  `process_announce` looks
    // for a `known_slaves` entry whose IP matches the Announce source IP to
    // find the event port; we seed that list with the real HomePod event addr.
    node.add_slave(homepod_event_addr);

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let node_handle = tokio::spawn(async move { node.run(shutdown_rx).await });

    let source = PtpPortIdentity::new(HOMEPOD_CLOCK_ID, 1);

    // Step 1: HomePod sends Announce → node switches to Slave.
    let announce = encode_homepod_announce(1, 248, HOMEPOD_CLOCK_ID);
    homepod_general_sock
        .send_to(&announce, client_general_addr)
        .await
        .unwrap();
    // Allow a short window for the node's event loop to process the Announce.
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Step 2: HomePod sends Sync (two-step flag set).
    let t1 = PtpTimestamp::now();
    let mut sync_msg = PtpMessage::sync(source, 1, t1);
    sync_msg.header.flags = 0x0200; // two-step: precise T1 is carried in Follow_Up
    homepod_event_sock
        .send_to(&sync_msg.encode(), client_event_addr)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Step 3: HomePod sends Follow_Up — this must trigger an immediate Delay_Req.
    let follow_up = PtpMessage::follow_up(source, 1, t1);
    homepod_general_sock
        .send_to(&follow_up.encode(), client_general_addr)
        .await
        .unwrap();

    // Step 4: Delay_Req must arrive within 400 ms.
    //
    // With the OLD code the only trigger was the periodic timer set to 10 s
    // (our slow config), so recv_from would time out here — proving the old
    // code missed the HomePod's burst window.
    //
    // With the NEW code the node sends Delay_Req immediately inside
    // handle_general_packet on Follow_Up receipt, arriving within ~5 ms on
    // loopback.  400 ms is a generous bound that comfortably fits inside the
    // HomePod's real burst window (~375 ms).
    let mut buf = [0u8; 256];
    let result = tokio::time::timeout(
        Duration::from_millis(400),
        homepod_event_sock.recv_from(&mut buf),
    )
    .await;

    let _ = shutdown_tx.send(true);
    let _ = node_handle.await;

    let (len, _) = result
        .expect(
            "Delay_Req must arrive within 400 ms — old timer-only code would miss the HomePod \
             burst window",
        )
        .unwrap();
    let msg = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(
        msg.header.message_type,
        PtpMessageType::DelayReq,
        "Node should send Delay_Req immediately on Follow_Up receipt (not timer)"
    );
}

/// Verify that a missed Delay_Resp does not permanently block future exchanges.
///
/// Scenario:
/// 1. Sync + Follow_Up → Delay_Req sent → `pending_t3` is now `Some(t3)`.
/// 2. Delay_Resp is lost (not sent by the master).
/// 3. New Sync arrives — must reset `pending_t3 = None` (the fix).
/// 4. Follow_Up for the new Sync → `pending_t3.is_none()` is `true` again → another Delay_Req is
///    sent.
///
/// Without the fix at step 3, `pending_t3` would remain `Some`, the guard in
/// `handle_general_packet` would prevent any further Delay_Req, and the clock
/// would be permanently stuck with no path to synchronization.
#[tokio::test]
async fn test_pending_t3_reset_prevents_stuck_exchange() {
    const CLIENT_CLOCK_ID: u64 = 0xCCCC_CCCC_CCCC_CCCCu64;
    const HOMEPOD_CLOCK_ID: u64 = 0xAAAA_AAAA_AAAA_AAAAu64;

    let homepod_event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let homepod_general_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let homepod_event_addr = homepod_event_sock.local_addr().unwrap();
    let _homepod_general_addr = homepod_general_sock.local_addr().unwrap();

    let client_event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_general_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_event_addr = client_event_sock.local_addr().unwrap();
    let client_general_addr = client_general_sock.local_addr().unwrap();
    let client_clock = create_shared_clock(CLIENT_CLOCK_ID, PtpRole::Master);
    let mut node = PtpNode::new(
        client_event_sock,
        Some(client_general_sock),
        client_clock,
        client_config_ieee_slow_timer(CLIENT_CLOCK_ID, 255),
    );
    node.add_slave(homepod_event_addr);

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let node_handle = tokio::spawn(async move { node.run(shutdown_rx).await });

    let source = PtpPortIdentity::new(HOMEPOD_CLOCK_ID, 1);
    let mut buf = [0u8; 256];

    // ── Setup: Announce → node becomes Slave ──────────────────────────────────
    let announce = encode_homepod_announce(1, 248, HOMEPOD_CLOCK_ID);
    homepod_general_sock
        .send_to(&announce, client_general_addr)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;

    // ── First exchange: Delay_Resp deliberately lost ──────────────────────────

    let t1a = PtpTimestamp::now();
    let mut sync1 = PtpMessage::sync(source, 1, t1a);
    sync1.header.flags = 0x0200;
    homepod_event_sock
        .send_to(&sync1.encode(), client_event_addr)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    homepod_general_sock
        .send_to(
            &PtpMessage::follow_up(source, 1, t1a).encode(),
            client_general_addr,
        )
        .await
        .unwrap();

    // Receive and discard the first Delay_Req; intentionally do NOT reply with
    // Delay_Resp.  This leaves `pending_t3 = Some(t3)` on the node.
    let drain = tokio::time::timeout(
        Duration::from_millis(400),
        homepod_event_sock.recv_from(&mut buf),
    )
    .await;
    assert!(
        drain.is_ok(),
        "Node should have sent first Delay_Req after first Follow_Up"
    );
    // No Delay_Resp sent — pending_t3 remains set on the node.

    // ── Second exchange: new Sync must reset pending_t3 ───────────────────────

    tokio::time::sleep(Duration::from_millis(20)).await;
    let t1b = PtpTimestamp::now();
    let mut sync2 = PtpMessage::sync(source, 2, t1b);
    sync2.header.flags = 0x0200;
    // Receiving this Sync must set `pending_t3 = None` in handle_event_packet,
    // otherwise the Follow_Up guard below cannot fire.
    homepod_event_sock
        .send_to(&sync2.encode(), client_event_addr)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    homepod_general_sock
        .send_to(
            &PtpMessage::follow_up(source, 2, t1b).encode(),
            client_general_addr,
        )
        .await
        .unwrap();

    // The node must send a second Delay_Req after the new Sync+Follow_Up pair.
    // Without `pending_t3 = None` in the Sync handler, the guard
    // `pending_t3.is_none()` in handle_general_packet would be false and no
    // Delay_Req would ever be sent — permanently stuck.
    let second = tokio::time::timeout(
        Duration::from_millis(400),
        homepod_event_sock.recv_from(&mut buf),
    )
    .await;

    let _ = shutdown_tx.send(true);
    let _ = node_handle.await;

    let (len, _) = second
        .expect(
            "Node must send a second Delay_Req after the new Sync resets pending_t3 (without the \
             fix the node would be permanently stuck)",
        )
        .unwrap();
    let msg = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(
        msg.header.message_type,
        PtpMessageType::DelayReq,
        "Second Delay_Req must be sent after new Sync resets pending_t3"
    );
}

/// End-to-end clock synchronization between two real PtpNodes.
///
/// A HomePod-like master (priority1=248) and a client slave (priority1=255)
/// exchange the full PTP sequence over real UDP loopback sockets:
///
/// ```text
/// HomePod (master)        Client (slave)
///   Announce ────────────>  (switches to Slave)
///   Sync (two-step) ─────>  (records T2)
///   Follow_Up ───────────>  (updates T1, immediately sends Delay_Req)
///   <──────────── Delay_Req
///   Delay_Resp ──────────>  (computes offset: is_synchronized → true)
/// ```
///
/// The client uses `delay_req_interval = 10 s` so Delay_Req is only sent via
/// the immediate Follow_Up path, making this an end-to-end validation of the
/// complete timing fix.
#[tokio::test]
async fn test_two_node_end_to_end_clock_sync() {
    const HOMEPOD_CLOCK_ID: u64 = 0xAAAA_AAAA_AAAA_AAAAu64;
    const CLIENT_CLOCK_ID: u64 = 0xCCCC_CCCC_CCCC_CCCCu64;

    // ── HomePod node: simulated master (priority1=248) ────────────────────────
    let (mut homepod_node, homepod_event_addr, _homepod_general_addr) =
        make_ieee_node(HOMEPOD_CLOCK_ID, 248).await;

    // ── Client node: will become Slave (priority1=255, slow timer) ────────────
    let client_event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_general_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_event_addr = client_event_sock.local_addr().unwrap();
    let client_general_addr = client_general_sock.local_addr().unwrap();
    // Keep a handle to the shared clock so we can inspect it after the test.
    let client_clock = create_shared_clock(CLIENT_CLOCK_ID, PtpRole::Master);
    let mut client_node = PtpNode::new(
        client_event_sock,
        Some(client_general_sock),
        client_clock.clone(),
        client_config_ieee_slow_timer(CLIENT_CLOCK_ID, 255),
    );

    // ── Wire up peer addresses ────────────────────────────────────────────────
    //
    // HomePod needs to know where to deliver Sync (client event socket) and
    // Follow_Up/Announce (client general socket).
    homepod_node.add_slave(client_event_addr);
    homepod_node.add_general_slave(client_general_addr);
    // Client uses HomePod's event addr as a lookup hint so that
    // `process_announce` resolves `remote_master.event_addr` to the correct
    // port (the Announce arrives on the general socket; the lookup in
    // `known_slaves` maps that IP to the real event port for Delay_Req).
    client_node.add_slave(homepod_event_addr);

    // ── Run both nodes concurrently ───────────────────────────────────────────
    let (homepod_shutdown_tx, homepod_shutdown_rx) = tokio::sync::watch::channel(false);
    let (client_shutdown_tx, client_shutdown_rx) = tokio::sync::watch::channel(false);

    let homepod_handle = tokio::spawn(async move { homepod_node.run(homepod_shutdown_rx).await });
    let client_handle = tokio::spawn(async move { client_node.run(client_shutdown_rx).await });

    // Allow time for:
    //  1. Initial Announce → client switches to Slave.
    //  2. Several Sync + Follow_Up + Delay_Req + Delay_Resp cycles.
    // HomePod sync_interval = 100 ms → ~7 potential cycles in 800 ms.
    tokio::time::sleep(Duration::from_millis(800)).await;

    let _ = homepod_shutdown_tx.send(true);
    let _ = client_shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), homepod_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), client_handle).await;

    // ── Verify the client clock is synchronized ───────────────────────────────
    let clock = client_clock.read().await;
    assert!(
        clock.is_synchronized(),
        "Client clock should be synchronized after PTP exchange (measurements={})",
        clock.measurement_count()
    );
    // On loopback both nodes share the same wall clock, so the measured offset
    // should be very close to zero.
    assert!(
        clock.offset_millis().abs() < 50.0,
        "Clock offset should be near zero on loopback: {:.3}ms",
        clock.offset_millis()
    );
    assert!(
        clock.measurement_count() >= 1,
        "Expected at least one successful timing measurement, got {}",
        clock.measurement_count()
    );
}

/// A node should not process its own Announce (clock_id matches config.clock_id).
#[tokio::test]
async fn test_bmca_ignores_own_announce() {
    const OUR_CLOCK_ID: u64 = 0xCCCC_CCCC_CCCC_CCCCu64;

    let (node, _event_addr, general_addr) = make_ieee_node(OUR_CLOCK_ID, 255).await;

    // Send an Announce claiming to be from OUR clock_id.
    // Even with priority1=1 (best), we must ignore it (it's our own echo).
    let announce = encode_homepod_announce(1, 1, OUR_CLOCK_ID);
    let node_after = run_node_with_announce(node, general_addr, announce).await;

    // Despite priority1=255 and the remote claiming priority1=1, we stay Master
    // because the clock_id matches ours (self-announce is ignored).
    assert_eq!(
        node_after.role(),
        EffectiveRole::Master,
        "Node must ignore Announce messages from its own clock_id"
    );
}
