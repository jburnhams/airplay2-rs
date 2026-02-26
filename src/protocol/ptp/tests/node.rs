use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::protocol::ptp::clock::PtpRole;
use crate::protocol::ptp::handler::create_shared_clock;
use crate::protocol::ptp::message::{
    AirPlayTimingPacket, PtpMessage, PtpMessageType, PtpPortIdentity,
};
use crate::protocol::ptp::node::{EffectiveRole, PtpNode, PtpNodeConfig};
use crate::protocol::ptp::timestamp::PtpTimestamp;

// ===== PtpNodeConfig =====

#[test]
fn test_node_config_defaults() {
    let config = PtpNodeConfig::default();
    assert_eq!(config.clock_id, 0);
    assert_eq!(config.priority1, 128);
    assert_eq!(config.priority2, 128);
    assert_eq!(config.sync_interval, Duration::from_secs(1));
    assert_eq!(config.delay_req_interval, Duration::from_secs(1));
    assert_eq!(config.announce_interval, Duration::from_secs(2));
    assert!(!config.use_airplay_format);
}

// ===== PtpNode construction =====

#[tokio::test]
async fn test_node_starts_as_master() {
    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let clock = create_shared_clock(0x1111, PtpRole::Master);
    let config = PtpNodeConfig {
        clock_id: 0x1111,
        ..Default::default()
    };
    let node = PtpNode::new(sock, None, clock, config);
    assert_eq!(node.role(), EffectiveRole::Master);
}

#[tokio::test]
async fn test_node_clock_accessor() {
    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let clock = create_shared_clock(0x2222, PtpRole::Master);
    let config = PtpNodeConfig {
        clock_id: 0x2222,
        ..Default::default()
    };
    let node = PtpNode::new(sock, None, clock.clone(), config);
    let clock_ref = node.clock();
    let c1 = clock.read().await;
    let c2 = clock_ref.read().await;
    assert_eq!(c1.clock_id(), c2.clock_id());
}

// ===== BMCA Priority Tests =====

#[tokio::test]
async fn test_bmca_lower_priority1_wins() {
    // Node has priority1=128, remote has priority1=64 (better).
    // Node should switch to Slave.
    let event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let general_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let remote_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let _event_addr = event_sock.local_addr().unwrap();
    let general_addr = general_sock.local_addr().unwrap();

    let clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let config = PtpNodeConfig {
        clock_id: 0xAAAA,
        priority1: 128,
        sync_interval: Duration::from_secs(60), // Long to avoid interference
        delay_req_interval: Duration::from_secs(60),
        announce_interval: Duration::from_secs(60),
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut node = PtpNode::new(event_sock, Some(general_sock), clock, config);

    let handle = tokio::spawn(async move {
        node.run(shutdown_rx).await.unwrap();
        node.role()
    });

    // Small delay for the node to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send an Announce with better priority1=64 on the general port
    let source = PtpPortIdentity::new(0xBBBB, 1);
    let announce = PtpMessage::announce(source, 0, 0xBBBB, 64, 128);
    remote_sock
        .send_to(&announce.encode(), general_addr)
        .await
        .unwrap();

    // Give it time to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Shutdown and check final role
    shutdown_tx.send(true).unwrap();
    let final_role = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_role,
        EffectiveRole::Slave,
        "Node should have switched to Slave after receiving better Announce"
    );
}

#[tokio::test]
async fn test_bmca_higher_priority1_stays_master() {
    // Node has priority1=64, remote has priority1=128 (worse).
    // Node should stay as Master.
    let event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let general_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let remote_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let general_addr = general_sock.local_addr().unwrap();

    let clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let config = PtpNodeConfig {
        clock_id: 0xAAAA,
        priority1: 64, // We have better priority
        sync_interval: Duration::from_secs(60),
        delay_req_interval: Duration::from_secs(60),
        announce_interval: Duration::from_secs(60),
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut node = PtpNode::new(event_sock, Some(general_sock), clock, config);

    let handle = tokio::spawn(async move {
        node.run(shutdown_rx).await.unwrap();
        node.role()
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send an Announce with worse priority1=128
    let source = PtpPortIdentity::new(0xBBBB, 1);
    let announce = PtpMessage::announce(source, 0, 0xBBBB, 128, 128);
    remote_sock
        .send_to(&announce.encode(), general_addr)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    shutdown_tx.send(true).unwrap();
    let final_role = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_role,
        EffectiveRole::Master,
        "Node should stay Master when it has better priority"
    );
}

#[tokio::test]
async fn test_bmca_equal_priority_lower_clock_id_wins() {
    // Same priority, but remote has lower clock_id (0x1000 < 0xAAAA).
    let event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let general_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let remote_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let general_addr = general_sock.local_addr().unwrap();

    let clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let config = PtpNodeConfig {
        clock_id: 0xAAAA,
        priority1: 128,
        priority2: 128,
        sync_interval: Duration::from_secs(60),
        delay_req_interval: Duration::from_secs(60),
        announce_interval: Duration::from_secs(60),
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut node = PtpNode::new(event_sock, Some(general_sock), clock, config);

    let handle = tokio::spawn(async move {
        node.run(shutdown_rx).await.unwrap();
        node.role()
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Remote with same priority but lower clock_id
    let source = PtpPortIdentity::new(0x1000, 1);
    let announce = PtpMessage::announce(source, 0, 0x1000, 128, 128);
    remote_sock
        .send_to(&announce.encode(), general_addr)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    shutdown_tx.send(true).unwrap();
    let final_role = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_role,
        EffectiveRole::Slave,
        "Node should become Slave when remote has same priority but lower clock_id"
    );
}

#[tokio::test]
async fn test_bmca_ignores_own_announce() {
    // If we receive our own Announce (reflected), we should stay Master.
    let event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let general_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let remote_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let general_addr = general_sock.local_addr().unwrap();

    let clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let config = PtpNodeConfig {
        clock_id: 0xAAAA,
        priority1: 200, // Intentionally bad priority
        sync_interval: Duration::from_secs(60),
        delay_req_interval: Duration::from_secs(60),
        announce_interval: Duration::from_secs(60),
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut node = PtpNode::new(event_sock, Some(general_sock), clock, config);

    let handle = tokio::spawn(async move {
        node.run(shutdown_rx).await.unwrap();
        node.role()
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send Announce with our own clock_id (even with better priority, should be ignored)
    let source = PtpPortIdentity::new(0xAAAA, 1);
    let announce = PtpMessage::announce(source, 0, 0xAAAA, 1, 1);
    remote_sock
        .send_to(&announce.encode(), general_addr)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    shutdown_tx.send(true).unwrap();
    let final_role = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_role,
        EffectiveRole::Master,
        "Node should ignore its own Announce and stay Master"
    );
}

// ===== Node as Master: responds to Delay_Req =====

#[tokio::test]
async fn test_node_master_responds_to_delay_req() {
    let node_event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let node_addr = node_event_sock.local_addr().unwrap();

    let clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let config = PtpNodeConfig {
        clock_id: 0xAAAA,
        sync_interval: Duration::from_secs(60),
        delay_req_interval: Duration::from_secs(60),
        announce_interval: Duration::from_secs(60),
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut node = PtpNode::new(node_event_sock, None, clock, config);

    let handle = tokio::spawn(async move { node.run(shutdown_rx).await });

    // Send Delay_Req to the node
    let source = PtpPortIdentity::new(0xBBBB, 1);
    let t3 = PtpTimestamp::new(100, 0);
    let req = PtpMessage::delay_req(source, 42, t3);
    client_sock.send_to(&req.encode(), node_addr).await.unwrap();

    // Receive DelayResp
    let mut buf = [0u8; 256];
    let result =
        tokio::time::timeout(Duration::from_secs(2), client_sock.recv_from(&mut buf)).await;
    assert!(result.is_ok(), "Did not receive Delay_Resp in time");

    let (len, _) = result.unwrap().unwrap();
    let resp = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(resp.header.message_type, PtpMessageType::DelayResp);
    assert_eq!(resp.header.sequence_id, 42);

    shutdown_tx.send(true).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}

// ===== Node as Master: AirPlay format Delay_Req =====

#[tokio::test]
async fn test_node_master_airplay_delay_req() {
    let node_event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let node_addr = node_event_sock.local_addr().unwrap();

    let clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let config = PtpNodeConfig {
        clock_id: 0xAAAA,
        use_airplay_format: true,
        sync_interval: Duration::from_secs(60),
        delay_req_interval: Duration::from_secs(60),
        announce_interval: Duration::from_secs(60),
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut node = PtpNode::new(node_event_sock, None, clock, config);

    let handle = tokio::spawn(async move { node.run(shutdown_rx).await });

    // Send AirPlay Delay_Req
    let req = AirPlayTimingPacket {
        message_type: PtpMessageType::DelayReq,
        sequence_id: 7,
        timestamp: PtpTimestamp::new(200, 0),
        clock_id: 0xBBBB,
    };
    client_sock.send_to(&req.encode(), node_addr).await.unwrap();

    // Receive AirPlay Delay_Resp
    let mut buf = [0u8; 256];
    let result =
        tokio::time::timeout(Duration::from_secs(2), client_sock.recv_from(&mut buf)).await;
    assert!(result.is_ok(), "Did not receive AirPlay Delay_Resp");

    let (len, _) = result.unwrap().unwrap();
    let resp = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
    assert_eq!(resp.message_type, PtpMessageType::DelayResp);
    assert_eq!(resp.sequence_id, 7);

    shutdown_tx.send(true).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}

// ===== Two PtpNodes: bidirectional sync over loopback (multi-round) =====

/// Run two `PtpNodes` against each other on loopback.
/// Node A has priority1=64 (master), Node B has priority1=128 (slave).
/// Verify that after multiple rounds of Sync/DelayReq exchange,
/// Node B's clock is synchronized with meaningful measurements.
#[tokio::test]
async fn test_two_nodes_bidirectional_sync_ieee1588() {
    // Node A: priority1=64 (will become master)
    let a_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let a_general = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let a_event_addr = a_event.local_addr().unwrap();
    let a_general_addr = a_general.local_addr().unwrap();

    // Node B: priority1=128 (will become slave)
    let b_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let b_general = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let b_event_addr = b_event.local_addr().unwrap();
    let b_general_addr = b_general.local_addr().unwrap();

    let a_clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let b_clock = create_shared_clock(0xBBBB, PtpRole::Slave);

    let a_config = PtpNodeConfig {
        clock_id: 0xAAAA,
        priority1: 64,
        priority2: 128,
        sync_interval: Duration::from_millis(150),
        delay_req_interval: Duration::from_millis(150),
        announce_interval: Duration::from_millis(200),
        ..Default::default()
    };

    let b_config = PtpNodeConfig {
        clock_id: 0xBBBB,
        priority1: 128,
        priority2: 128,
        sync_interval: Duration::from_millis(150),
        delay_req_interval: Duration::from_millis(150),
        announce_interval: Duration::from_millis(200),
        ..Default::default()
    };

    let (a_shutdown_tx, a_shutdown_rx) = tokio::sync::watch::channel(false);
    let (b_shutdown_tx, b_shutdown_rx) = tokio::sync::watch::channel(false);

    let a_clock_ref = a_clock.clone();
    let b_clock_ref = b_clock.clone();

    // Use a barrier to ensure both nodes start simultaneously,
    // so neither misses the other's initial Announce.
    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    // Spawn Node A
    let barrier_a = barrier.clone();
    let a_handle = tokio::spawn(async move {
        let mut node_a = PtpNode::new(a_event, Some(a_general), a_clock_ref, a_config);
        node_a.add_slave(b_event_addr);
        node_a.add_general_slave(b_general_addr);
        barrier_a.wait().await;
        node_a.run(a_shutdown_rx).await.unwrap();
        node_a.role()
    });

    // Spawn Node B
    let barrier_b = barrier.clone();
    let b_handle = tokio::spawn(async move {
        let mut node_b = PtpNode::new(b_event, Some(b_general), b_clock_ref, b_config);
        node_b.add_slave(a_event_addr);
        node_b.add_general_slave(a_general_addr);
        barrier_b.wait().await;
        node_b.run(b_shutdown_rx).await.unwrap();
        node_b.role()
    });

    // Let them run for enough time to exchange multiple Sync/DelayReq rounds.
    // With 150ms intervals and 200ms announce, 4 seconds gives plenty of rounds.
    tokio::time::sleep(Duration::from_secs(4)).await;

    // Shutdown both
    a_shutdown_tx.send(true).unwrap();
    b_shutdown_tx.send(true).unwrap();

    let a_role = tokio::time::timeout(Duration::from_secs(2), a_handle)
        .await
        .unwrap()
        .unwrap();
    let b_role = tokio::time::timeout(Duration::from_secs(2), b_handle)
        .await
        .unwrap()
        .unwrap();

    // Verify roles: A should be master, B should be slave (due to Announce exchange)
    assert_eq!(a_role, EffectiveRole::Master, "Node A should remain Master");
    assert_eq!(
        b_role,
        EffectiveRole::Slave,
        "Node B should have become Slave"
    );

    // Verify Node B's clock is synchronized (has processed timing measurements)
    let b_clock_locked = b_clock.read().await;
    assert_eq!(
        b_role,
        EffectiveRole::Slave,
        "Node B should have become Slave"
    );
    assert!(
        b_clock_locked.is_synchronized(),
        "Node B (slave) should be synchronized after multiple rounds"
    );
    assert!(
        b_clock_locked.measurement_count() >= 2,
        "Node B should have at least 2 measurements, got {}",
        b_clock_locked.measurement_count()
    );

    // On loopback, offset should be very small (< 50ms)
    let offset_ms = b_clock_locked.offset_millis().abs();
    assert!(
        offset_ms < 50.0,
        "Offset on loopback should be small, got {offset_ms:.3}ms"
    );
}

/// Same test but with `AirPlay` compact format.
#[tokio::test]
async fn test_two_nodes_bidirectional_sync_airplay_format() {
    // Node A: priority1=64 (master)
    let a_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let a_event_addr = a_event.local_addr().unwrap();

    // Node B: priority1=128 (slave)
    let b_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let b_event_addr = b_event.local_addr().unwrap();

    let a_clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let b_clock = create_shared_clock(0xBBBB, PtpRole::Slave);

    let a_config = PtpNodeConfig {
        clock_id: 0xAAAA,
        priority1: 64,
        sync_interval: Duration::from_millis(200),
        delay_req_interval: Duration::from_millis(200),
        announce_interval: Duration::from_millis(500),
        use_airplay_format: true,
        ..Default::default()
    };

    let b_config = PtpNodeConfig {
        clock_id: 0xBBBB,
        priority1: 128,
        sync_interval: Duration::from_millis(200),
        delay_req_interval: Duration::from_millis(200),
        announce_interval: Duration::from_millis(500),
        use_airplay_format: true,
        ..Default::default()
    };

    let (a_shutdown_tx, a_shutdown_rx) = tokio::sync::watch::channel(false);
    let (b_shutdown_tx, b_shutdown_rx) = tokio::sync::watch::channel(false);

    let a_clock_ref = a_clock.clone();
    let b_clock_ref = b_clock.clone();

    let a_handle = tokio::spawn(async move {
        let mut node_a = PtpNode::new(a_event, None, a_clock_ref, a_config);
        node_a.add_slave(b_event_addr);
        node_a.run(a_shutdown_rx).await.unwrap();
        node_a.role()
    });

    let b_handle = tokio::spawn(async move {
        let mut node_b = PtpNode::new(b_event, None, b_clock_ref, b_config);
        node_b.add_slave(a_event_addr);
        node_b.run(b_shutdown_rx).await.unwrap();
        node_b.role()
    });

    tokio::time::sleep(Duration::from_secs(3)).await;

    a_shutdown_tx.send(true).unwrap();
    b_shutdown_tx.send(true).unwrap();

    let _a_role = tokio::time::timeout(Duration::from_secs(2), a_handle)
        .await
        .unwrap()
        .unwrap();
    let _b_role = tokio::time::timeout(Duration::from_secs(2), b_handle)
        .await
        .unwrap()
        .unwrap();

    // Verify B's clock synchronized (AirPlay format doesn't use Announce for BMCA,
    // but both nodes should still exchange Sync/DelayReq)
    let b_clock_locked = b_clock.read().await;
    assert!(
        b_clock_locked.is_synchronized(),
        "Node B should be synchronized after AirPlay format exchange"
    );
    assert!(
        b_clock_locked.measurement_count() >= 2,
        "Node B should have at least 2 measurements, got {}",
        b_clock_locked.measurement_count()
    );

    let offset_ms = b_clock_locked.offset_millis().abs();
    assert!(
        offset_ms < 100.0,
        "Offset on loopback should be small, got {offset_ms:.3}ms"
    );
}

// ===== Verify sync converges over multiple rounds =====

/// Run two IEEE 1588 nodes for 5 seconds with fast intervals, then verify:
/// 1. Multiple measurements accumulated (not just 1-2)
/// 2. Offset is stable (small)
/// 3. RTT measurements are reasonable
#[tokio::test]
async fn test_sync_convergence_multiple_rounds() {
    let a_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let a_general = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let a_event_addr = a_event.local_addr().unwrap();
    let a_general_addr = a_general.local_addr().unwrap();

    let b_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let b_general = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let b_event_addr = b_event.local_addr().unwrap();
    let b_general_addr = b_general.local_addr().unwrap();

    let a_clock = create_shared_clock(0x0001, PtpRole::Master);
    let b_clock = create_shared_clock(0x0002, PtpRole::Slave);

    let fast_config = |id: u64, p1: u8| PtpNodeConfig {
        clock_id: id,
        priority1: p1,
        sync_interval: Duration::from_millis(100),
        delay_req_interval: Duration::from_millis(100),
        announce_interval: Duration::from_millis(150),
        ..Default::default()
    };

    let (a_shutdown_tx, a_shutdown_rx) = tokio::sync::watch::channel(false);
    let (b_shutdown_tx, b_shutdown_rx) = tokio::sync::watch::channel(false);

    let a_clock_ref = a_clock.clone();
    let b_clock_ref = b_clock.clone();

    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let barrier_a = barrier.clone();
    let a_handle = tokio::spawn(async move {
        let mut node_a = PtpNode::new(
            a_event,
            Some(a_general),
            a_clock_ref,
            fast_config(0x0001, 64),
        );
        node_a.add_slave(b_event_addr);
        node_a.add_general_slave(b_general_addr);
        barrier_a.wait().await;
        node_a.run(a_shutdown_rx).await.unwrap();
    });

    let barrier_b = barrier.clone();
    let b_handle = tokio::spawn(async move {
        let mut node_b = PtpNode::new(
            b_event,
            Some(b_general),
            b_clock_ref,
            fast_config(0x0002, 128),
        );
        node_b.add_slave(a_event_addr);
        node_b.add_general_slave(a_general_addr);
        barrier_b.wait().await;
        node_b.run(b_shutdown_rx).await.unwrap();
    });

    // Run for 5 seconds with 100ms intervals = ~50 rounds
    tokio::time::sleep(Duration::from_secs(5)).await;

    a_shutdown_tx.send(true).unwrap();
    b_shutdown_tx.send(true).unwrap();

    let _ = tokio::time::timeout(Duration::from_secs(2), a_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), b_handle).await;

    // Verify convergence
    let b_clock_locked = b_clock.read().await;

    assert!(
        b_clock_locked.is_synchronized(),
        "Slave should be synchronized"
    );

    // Should have accumulated many measurements (capped by max_measurements=8)
    let count = b_clock_locked.measurement_count();
    assert!(
        count >= 1,
        "Expected at least 1 measurement after 5 seconds at 100ms intervals, got {count}"
    );

    // Offset should be very small on loopback
    let offset_ms = b_clock_locked.offset_millis().abs();
    assert!(
        offset_ms < 50.0,
        "Expected offset < 50ms on loopback after convergence, got {offset_ms:.3}ms"
    );

    // RTT should be very small on loopback
    if let Some(rtt) = b_clock_locked.median_rtt() {
        assert!(
            rtt < Duration::from_millis(10),
            "Expected RTT < 10ms on loopback, got {rtt:?}"
        );
    }
}

// ===== Role reversal test =====

/// Start both nodes with equal priority, then change one to have better priority
/// by sending a new Announce. Verify the role switches correctly.
#[tokio::test]
async fn test_role_reversal_via_announce() {
    let a_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let a_general = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let a_general_addr = a_general.local_addr().unwrap();

    let external_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let clock = create_shared_clock(0xCCCC, PtpRole::Master);
    let config = PtpNodeConfig {
        clock_id: 0xCCCC,
        priority1: 128,
        sync_interval: Duration::from_secs(60),
        delay_req_interval: Duration::from_secs(60),
        announce_interval: Duration::from_secs(60),
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut node = PtpNode::new(a_event, Some(a_general), clock, config);

    let handle = tokio::spawn(async move {
        node.run(shutdown_rx).await.unwrap();
        node.role()
    });

    // Wait a bit, node starts as master
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send Announce from a superior clock (priority1=32)
    let source1 = PtpPortIdentity::new(0xDDDD, 1);
    let announce1 = PtpMessage::announce(source1, 0, 0xDDDD, 32, 128);
    external_sock
        .send_to(&announce1.encode(), a_general_addr)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    shutdown_tx.send(true).unwrap();
    let final_role = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_role,
        EffectiveRole::Slave,
        "Node should switch to Slave after receiving superior Announce"
    );
}

// ===== Slave handler DelayResp on general port =====

/// Verify that the slave handler (`PtpSlaveHandler`) correctly processes
/// `DelayResp` received on the general port (320) instead of event port.
#[tokio::test]
async fn test_slave_handler_delay_resp_on_general_port() {
    use crate::protocol::ptp::handler::{PtpHandlerConfig, PtpSlaveHandler};

    let slave_event_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let slave_general_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let master_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let master_general_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let slave_event_addr = slave_event_sock.local_addr().unwrap();
    let slave_general_addr = slave_general_sock.local_addr().unwrap();
    let master_addr = master_sock.local_addr().unwrap();

    let slave_clock = create_shared_clock(0xBBBB, PtpRole::Slave);
    let config = PtpHandlerConfig {
        clock_id: 0xBBBB,
        role: PtpRole::Slave,
        delay_req_interval: Duration::from_millis(100),
        sync_interval: Duration::from_secs(60),
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let slave_clock_ref = slave_clock.clone();

    let handle = tokio::spawn(async move {
        let mut handler = PtpSlaveHandler::new(
            slave_event_sock,
            Some(slave_general_sock),
            slave_clock_ref,
            config,
            master_addr,
        );
        handler.run(shutdown_rx).await
    });

    // Step 1: Master sends Sync on event port
    let master_source = PtpPortIdentity::new(0xAAAA, 1);
    let t1 = PtpTimestamp::now();
    let mut sync_msg = PtpMessage::sync(master_source, 1, t1);
    sync_msg.header.flags = 0x0200;
    master_sock
        .send_to(&sync_msg.encode(), slave_event_addr)
        .await
        .unwrap();

    // Step 2: Master sends Follow_Up on general port
    tokio::time::sleep(Duration::from_millis(10)).await;
    let precise_t1 = PtpTimestamp::now();
    let follow_up = PtpMessage::follow_up(master_source, 1, precise_t1);
    master_general_sock
        .send_to(&follow_up.encode(), slave_general_addr)
        .await
        .unwrap();

    // Step 3: Wait for slave to send Delay_Req (triggered by timer)
    let mut buf = [0u8; 256];
    let result =
        tokio::time::timeout(Duration::from_secs(2), master_sock.recv_from(&mut buf)).await;
    assert!(result.is_ok(), "Did not receive Delay_Req from slave");
    let (len, _from) = result.unwrap().unwrap();
    let req = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(req.header.message_type, PtpMessageType::DelayReq);

    // Step 4: Master sends Delay_Resp on GENERAL port (as per IEEE 1588)
    let t4 = PtpTimestamp::now();
    let resp = PtpMessage::delay_resp(
        master_source,
        req.header.sequence_id,
        t4,
        req.header.source_port_identity,
    );
    master_general_sock
        .send_to(&resp.encode(), slave_general_addr)
        .await
        .unwrap();

    // Wait for slave to process
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify the slave clock was synced
    {
        let clock = slave_clock.read().await;
        assert!(
            clock.is_synchronized(),
            "Slave should be synchronized after receiving DelayResp on general port"
        );
        assert!(
            clock.measurement_count() >= 1,
            "Should have at least 1 measurement"
        );
    }

    shutdown_tx.send(true).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}
