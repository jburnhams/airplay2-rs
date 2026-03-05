//! Integration test for PTP timing synchronization using PtpNode.
//!
//! Tests that two PtpNodes can connect, discover roles via BMCA, and achieve
//! bidirectional clock synchronization over multiple rounds — verifying that
//! sync actually converges rather than just sending a couple of packets.

use std::sync::Arc;
use std::time::Duration;

use airplay2::protocol::ptp::clock::PtpRole;
use airplay2::protocol::ptp::handler::create_shared_clock;
use airplay2::protocol::ptp::node::{EffectiveRole, PtpNode, PtpNodeConfig};
use tokio::net::UdpSocket;

/// Simulate the Kitchen HomePod as PTP grandmaster (priority1=64, best clock)
/// and the client as slave (priority1=128). Verify that after running for
/// several seconds, the client's clock synchronizes with multiple measurements.
#[tokio::test]
async fn test_kitchen_device_ptp_sync() {
    // "Kitchen" device: priority1=64 (grandmaster)
    let kitchen_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let kitchen_general = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let kitchen_event_addr = kitchen_event.local_addr().unwrap();
    let kitchen_general_addr = kitchen_general.local_addr().unwrap();

    // Client: priority1=128 (slave)
    let client_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_general = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_event_addr = client_event.local_addr().unwrap();
    let client_general_addr = client_general.local_addr().unwrap();

    let kitchen_clock = create_shared_clock(0x4B49_5443_4845_4E00, PtpRole::Master); // "KITCHEN\0"
    let client_clock = create_shared_clock(0x434C_4945_4E54_0000, PtpRole::Slave); // "CLIENT\0\0"

    let kitchen_config = PtpNodeConfig {
        clock_id: 0x4B49_5443_4845_4E00,
        priority1: 64,
        priority2: 128,
        sync_interval: Duration::from_millis(100),
        delay_req_interval: Duration::from_millis(100),
        announce_interval: Duration::from_millis(200),
        ..Default::default()
    };

    let client_config = PtpNodeConfig {
        clock_id: 0x434C_4945_4E54_0000,
        priority1: 128,
        priority2: 128,
        sync_interval: Duration::from_millis(100),
        delay_req_interval: Duration::from_millis(100),
        announce_interval: Duration::from_millis(200),
        ..Default::default()
    };

    let (kitchen_shutdown_tx, kitchen_shutdown_rx) = tokio::sync::watch::channel(false);
    let (client_shutdown_tx, client_shutdown_rx) = tokio::sync::watch::channel(false);

    let kitchen_clock_ref = kitchen_clock.clone();
    let client_clock_ref = client_clock.clone();

    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let barrier_k = barrier.clone();
    let kitchen_handle = tokio::spawn(async move {
        let mut node = PtpNode::new(
            kitchen_event,
            Some(kitchen_general),
            kitchen_clock_ref,
            kitchen_config,
        );
        node.add_slave(client_event_addr);
        node.add_general_slave(client_general_addr);
        barrier_k.wait().await;
        node.run(kitchen_shutdown_rx).await.unwrap();
        node.role()
    });

    let barrier_c = barrier.clone();
    let client_handle = tokio::spawn(async move {
        let mut node = PtpNode::new(
            client_event,
            Some(client_general),
            client_clock_ref,
            client_config,
        );
        node.add_slave(kitchen_event_addr);
        node.add_general_slave(kitchen_general_addr);
        barrier_c.wait().await;
        node.run(client_shutdown_rx).await.unwrap();
        node.role()
    });

    // Run for 5 seconds — enough for many sync rounds
    tokio::time::sleep(Duration::from_secs(5)).await;

    kitchen_shutdown_tx.send(true).unwrap();
    client_shutdown_tx.send(true).unwrap();

    let kitchen_role = tokio::time::timeout(Duration::from_secs(2), kitchen_handle)
        .await
        .unwrap()
        .unwrap();
    let client_role = tokio::time::timeout(Duration::from_secs(2), client_handle)
        .await
        .unwrap()
        .unwrap();

    // Verify roles
    assert_eq!(
        kitchen_role,
        EffectiveRole::Master,
        "Kitchen should be Master (better priority)"
    );
    assert_eq!(client_role, EffectiveRole::Slave, "Client should be Slave");

    // Verify client clock converged
    let client_clk = client_clock.read().await;
    assert!(
        client_clk.is_synchronized(),
        "Client should be synchronized with Kitchen"
    );
    let count = client_clk.measurement_count();
    assert!(
        count >= 3,
        "Client should have multiple measurements (got {count})"
    );
    let offset = client_clk.offset_millis().abs();
    assert!(
        offset < 10.0,
        "Offset should be very small on loopback (got {offset:.3}ms)"
    );
    if let Some(rtt) = client_clk.median_rtt() {
        assert!(
            rtt < Duration::from_millis(5),
            "Median RTT should be tiny on loopback (got {rtt:?})"
        );
    }
}

/// Simulate two AirPlay devices (Kitchen and Bedroom) both as potential masters.
/// Kitchen has priority1=64 (better), Bedroom has priority1=96.
/// Both should negotiate roles: Kitchen=master, Bedroom=slave.
/// Verify Bedroom synchronizes its clock to Kitchen.
#[tokio::test]
async fn test_kitchen_bedroom_bmca_negotiation() {
    // Kitchen: priority1=64
    let kitchen_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let kitchen_general = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let kitchen_event_addr = kitchen_event.local_addr().unwrap();
    let kitchen_general_addr = kitchen_general.local_addr().unwrap();

    // Bedroom: priority1=96
    let bedroom_event = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let bedroom_general = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let bedroom_event_addr = bedroom_event.local_addr().unwrap();
    let bedroom_general_addr = bedroom_general.local_addr().unwrap();

    let kitchen_clock = create_shared_clock(0x4B49_5443_0001, PtpRole::Master);
    let bedroom_clock = create_shared_clock(0x4245_4452_0002, PtpRole::Master);

    let mk = |id, p1| PtpNodeConfig {
        clock_id: id,
        priority1: p1,
        priority2: 128,
        sync_interval: Duration::from_millis(100),
        delay_req_interval: Duration::from_millis(100),
        announce_interval: Duration::from_millis(150),
        ..Default::default()
    };

    let (k_shutdown_tx, k_shutdown_rx) = tokio::sync::watch::channel(false);
    let (b_shutdown_tx, b_shutdown_rx) = tokio::sync::watch::channel(false);

    let kitchen_clock_ref = kitchen_clock.clone();
    let bedroom_clock_ref = bedroom_clock.clone();

    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let barrier_k = barrier.clone();
    let k_handle = tokio::spawn(async move {
        let mut node = PtpNode::new(
            kitchen_event,
            Some(kitchen_general),
            kitchen_clock_ref,
            mk(0x4B49_5443_0001, 64),
        );
        node.add_slave(bedroom_event_addr);
        node.add_general_slave(bedroom_general_addr);
        barrier_k.wait().await;
        node.run(k_shutdown_rx).await.unwrap();
        node.role()
    });

    let barrier_b = barrier.clone();
    let b_handle = tokio::spawn(async move {
        let mut node = PtpNode::new(
            bedroom_event,
            Some(bedroom_general),
            bedroom_clock_ref,
            mk(0x4245_4452_0002, 96),
        );
        node.add_slave(kitchen_event_addr);
        node.add_general_slave(kitchen_general_addr);
        barrier_b.wait().await;
        node.run(b_shutdown_rx).await.unwrap();
        node.role()
    });

    tokio::time::sleep(Duration::from_secs(5)).await;

    k_shutdown_tx.send(true).unwrap();
    b_shutdown_tx.send(true).unwrap();

    let k_role = tokio::time::timeout(Duration::from_secs(2), k_handle)
        .await
        .unwrap()
        .unwrap();
    let b_role = tokio::time::timeout(Duration::from_secs(2), b_handle)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(k_role, EffectiveRole::Master, "Kitchen should be Master");
    assert_eq!(b_role, EffectiveRole::Slave, "Bedroom should be Slave");

    // Bedroom clock should be synced
    let bed_clk = bedroom_clock.read().await;
    assert!(
        bed_clk.is_synchronized(),
        "Bedroom should be synchronized with Kitchen"
    );
    assert!(
        bed_clk.measurement_count() >= 3,
        "Bedroom should have multiple sync measurements (got {})",
        bed_clk.measurement_count()
    );
    let offset = bed_clk.offset_millis().abs();
    assert!(
        offset < 10.0,
        "Bedroom offset should be small (got {offset:.3}ms)"
    );
}

/// Test three nodes: Client, Kitchen, Bedroom.
/// Client priority1=128, Kitchen priority1=64, Bedroom priority1=96.
/// Kitchen should be grandmaster. Both Client and Bedroom should sync to it.
#[tokio::test]
async fn test_three_node_multi_room_sync() {
    let mk_pair = || async {
        (
            Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
            Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap()),
        )
    };

    let (kitchen_event, kitchen_general) = mk_pair().await;
    let (bedroom_event, bedroom_general) = mk_pair().await;
    let (client_event, client_general) = mk_pair().await;

    let ke = kitchen_event.local_addr().unwrap();
    let kg = kitchen_general.local_addr().unwrap();
    let be = bedroom_event.local_addr().unwrap();
    let bg = bedroom_general.local_addr().unwrap();
    let ce = client_event.local_addr().unwrap();
    let cg = client_general.local_addr().unwrap();

    let kitchen_clock = create_shared_clock(0x0001, PtpRole::Master);
    let bedroom_clock = create_shared_clock(0x0002, PtpRole::Master);
    let client_clock = create_shared_clock(0x0003, PtpRole::Master);

    let mk_config = |id, p1| PtpNodeConfig {
        clock_id: id,
        priority1: p1,
        priority2: 128,
        sync_interval: Duration::from_millis(100),
        delay_req_interval: Duration::from_millis(100),
        announce_interval: Duration::from_millis(150),
        ..Default::default()
    };

    let (k_tx, k_rx) = tokio::sync::watch::channel(false);
    let (b_tx, b_rx) = tokio::sync::watch::channel(false);
    let (c_tx, c_rx) = tokio::sync::watch::channel(false);

    let barrier = Arc::new(tokio::sync::Barrier::new(3));

    // Kitchen: knows about bedroom and client
    let kc = kitchen_clock.clone();
    let bk = barrier.clone();
    let k_handle = tokio::spawn(async move {
        let mut n = PtpNode::new(
            kitchen_event,
            Some(kitchen_general),
            kc,
            mk_config(0x0001, 64),
        );
        n.add_slave(be);
        n.add_general_slave(bg);
        n.add_slave(ce);
        n.add_general_slave(cg);
        bk.wait().await;
        n.run(k_rx).await.unwrap();
        n.role()
    });

    // Bedroom: knows about kitchen and client
    let bc = bedroom_clock.clone();
    let bb = barrier.clone();
    let b_handle = tokio::spawn(async move {
        let mut n = PtpNode::new(
            bedroom_event,
            Some(bedroom_general),
            bc,
            mk_config(0x0002, 96),
        );
        n.add_slave(ke);
        n.add_general_slave(kg);
        n.add_slave(ce);
        n.add_general_slave(cg);
        bb.wait().await;
        n.run(b_rx).await.unwrap();
        n.role()
    });

    // Client: knows about kitchen and bedroom
    let cc = client_clock.clone();
    let cb = barrier.clone();
    let c_handle = tokio::spawn(async move {
        let mut n = PtpNode::new(
            client_event,
            Some(client_general),
            cc,
            mk_config(0x0003, 128),
        );
        n.add_slave(ke);
        n.add_general_slave(kg);
        n.add_slave(be);
        n.add_general_slave(bg);
        cb.wait().await;
        n.run(c_rx).await.unwrap();
        n.role()
    });

    tokio::time::sleep(Duration::from_secs(5)).await;

    k_tx.send(true).unwrap();
    b_tx.send(true).unwrap();
    c_tx.send(true).unwrap();

    let k_role = tokio::time::timeout(Duration::from_secs(2), k_handle)
        .await
        .unwrap()
        .unwrap();
    let b_role = tokio::time::timeout(Duration::from_secs(2), b_handle)
        .await
        .unwrap()
        .unwrap();
    let c_role = tokio::time::timeout(Duration::from_secs(2), c_handle)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        k_role,
        EffectiveRole::Master,
        "Kitchen should be grandmaster"
    );
    assert_eq!(b_role, EffectiveRole::Slave, "Bedroom should be slave");
    assert_eq!(c_role, EffectiveRole::Slave, "Client should be slave");

    // Both bedroom and client should be synced
    let bed = bedroom_clock.read().await;
    assert!(bed.is_synchronized(), "Bedroom should be synced");
    assert!(
        bed.measurement_count() >= 3,
        "Bedroom should have >= 3 measurements (got {})",
        bed.measurement_count()
    );

    let cli = client_clock.read().await;
    assert!(cli.is_synchronized(), "Client should be synced");
    assert!(
        cli.measurement_count() >= 3,
        "Client should have >= 3 measurements (got {})",
        cli.measurement_count()
    );
}
