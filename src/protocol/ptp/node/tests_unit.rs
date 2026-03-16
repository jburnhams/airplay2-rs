use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crate::protocol::ptp::clock::PtpRole;
use crate::protocol::ptp::handler::create_shared_clock;
use crate::protocol::ptp::node::{EffectiveRole, PtpNode, PtpNodeConfig};

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
    let node = make_node(255, 0xAAAA).await;
    assert!(
        node.compare_priority(128, 128, 0xBBBB),
        "Remote with lower priority1 must win"
    );
}

#[tokio::test]
async fn test_compare_priority_we_win_with_lower_p1() {
    let node = make_node(64, 0xAAAA).await;
    assert!(
        !node.compare_priority(128, 128, 0xBBBB),
        "Remote with higher priority1 must NOT win"
    );
}

#[tokio::test]
async fn test_compare_priority_equal_p1_remote_wins_lower_p2() {
    let node = make_node(128, 0xAAAA).await;
    assert!(
        node.compare_priority(128, 64, 0xBBBB),
        "Remote with lower priority2 (tie on p1) must win"
    );
}

#[tokio::test]
async fn test_compare_priority_equal_p1_we_win_higher_remote_p2() {
    let node = make_node(128, 0xAAAA).await;
    assert!(
        !node.compare_priority(128, 200, 0xBBBB),
        "Remote with higher priority2 (tie on p1) must NOT win"
    );
}

#[tokio::test]
async fn test_compare_priority_tiebreak_on_lower_clock_id() {
    let node = make_node(128, 0xAAAA).await;
    assert!(
        node.compare_priority(128, 128, 0x0001),
        "Remote with lower clock_id (tie on both priorities) must win"
    );
}

#[tokio::test]
async fn test_compare_priority_tiebreak_on_higher_clock_id_we_win() {
    let node = make_node(128, 0xAAAA).await;
    assert!(
        !node.compare_priority(128, 128, 0xFFFF),
        "Remote with higher clock_id (tie on both priorities) must NOT win"
    );
}

#[tokio::test]
async fn test_compare_priority_identical_parameters_is_false() {
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
    let our_clock_id = 0xAAAA_BBBB_CCCC_DDDD;
    let mut node = make_node(255, our_clock_id).await;
    let src = SocketAddr::from_str("192.168.1.100:320").unwrap();

    node.process_announce(our_clock_id, 1, 1, src);

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
    let mut node = make_node(64, 0xAAAA).await;

    let src = SocketAddr::from_str("192.168.1.100:320").unwrap();
    node.process_announce(0xBBBB, 128, 128, src);
    assert!(node.remote_master.is_none());

    node.role = EffectiveRole::Slave;
    node.remote_master = Some(crate::protocol::ptp::node::RemoteMaster {
        grandmaster_identity: 0xBBBB,
        priority1: 128,
        priority2: 128,
        event_addr: SocketAddr::from_str("192.168.1.100:319").unwrap(),
        general_addr: src,
        last_announce: tokio::time::Instant::now(),
    });
    node.config.priority1 = 64;

    node.process_announce(0xBBBB, 128, 128, src);
    assert!(
        node.remote_master.is_some(),
        "remote_master entry must be preserved when remote sends a new Announce"
    );
}

// ── check_announce_timeout ────────────────────────────────────────────────

#[tokio::test]
async fn test_announce_timeout_reverts_to_master() {
    let mut node = make_node(255, 0xAAAA).await;

    let src = SocketAddr::from_str("192.168.1.100:320").unwrap();
    node.process_announce(0xBBBB, 128, 128, src);
    assert_eq!(node.role, EffectiveRole::Slave);

    node.announce_timeout = Duration::from_nanos(1);

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
    let mut node = make_node(128, 0xAAAA).await;
    assert_eq!(node.role, EffectiveRole::Master);
    assert!(node.remote_master.is_none());

    node.announce_timeout = Duration::from_nanos(1);
    tokio::time::sleep(Duration::from_millis(5)).await;
    node.check_announce_timeout(); // must not panic

    assert_eq!(node.role, EffectiveRole::Master);
}

// ── Delay_Req timeout / retry (DELAY_REQ_TIMEOUT) ────────────────────────

#[test]
fn test_delay_req_timeout_constant_is_one_second() {
    assert_eq!(
        crate::protocol::ptp::node::DELAY_REQ_TIMEOUT,
        Duration::from_secs(1),
        "DELAY_REQ_TIMEOUT must be 1 second"
    );
}
