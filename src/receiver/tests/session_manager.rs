use crate::receiver::session_manager::{
    PreemptionPolicy, SessionEvent, SessionManager, SessionManagerConfig,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

fn test_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)
}

fn test_addr_2() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346)
}

#[tokio::test]
async fn test_start_session() {
    let config = SessionManagerConfig::default();
    let manager = SessionManager::new(config);

    assert!(!manager.has_active_session().await);

    let id = manager.start_session(test_addr()).await.unwrap();
    assert!(manager.has_active_session().await);
    assert_eq!(manager.current_session_id().await, Some(id));
}

#[tokio::test]
async fn test_preemption_allow() {
    let config = SessionManagerConfig {
        preemption_policy: PreemptionPolicy::AllowPreempt,
        ..Default::default()
    };
    let manager = SessionManager::new(config);

    let id1 = manager.start_session(test_addr()).await.unwrap();

    // Should succeed and replace id1
    let id2 = manager.start_session(test_addr_2()).await.unwrap();

    assert_ne!(id1, id2);
    assert_eq!(manager.current_session_id().await, Some(id2));
}

#[tokio::test]
async fn test_preemption_reject() {
    let config = SessionManagerConfig {
        preemption_policy: PreemptionPolicy::Reject,
        ..Default::default()
    };
    let manager = SessionManager::new(config);

    let _id1 = manager.start_session(test_addr()).await.unwrap();

    // Should fail
    let result = manager.start_session(test_addr_2()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_socket_allocation() {
    let config = SessionManagerConfig::default();
    let manager = SessionManager::new(config);

    manager.start_session(test_addr()).await.unwrap();

    let (ap, cp, tp) = manager.allocate_sockets().await.unwrap();
    assert!(ap > 0);
    assert!(cp > 0);
    assert!(tp > 0);

    // Check sockets are stored
    let sockets_lock = manager.get_sockets().unwrap();
    let sockets = sockets_lock.lock().await;
    assert!(sockets.is_some());
}

#[tokio::test]
async fn test_session_cleanup() {
    let config = SessionManagerConfig::default();
    let manager = SessionManager::new(config);

    manager.start_session(test_addr()).await.unwrap();
    manager.allocate_sockets().await.unwrap();

    manager.end_session("Test cleanup").await;

    assert!(!manager.has_active_session().await);

    let sockets_lock = manager.get_sockets().unwrap();
    let sockets = sockets_lock.lock().await;
    assert!(sockets.is_none());
}

#[tokio::test]
async fn test_idle_timeout() {
    let config = SessionManagerConfig {
        idle_timeout: Duration::from_millis(100),
        ..Default::default()
    };
    let manager = Arc::new(SessionManager::new(config));

    manager.start_session(test_addr()).await.unwrap();

    // Not timed out yet
    assert!(!manager.check_timeout().await);

    // Wait for timeout
    sleep(Duration::from_millis(150)).await;

    // Should be timed out
    assert!(manager.check_timeout().await);

    // Enforce timeout
    manager.enforce_timeouts().await;
    assert!(!manager.has_active_session().await);
}

#[tokio::test]
async fn test_volume_events() {
    let config = SessionManagerConfig::default();
    let manager = SessionManager::new(config);
    let mut rx = manager.subscribe();

    manager.start_session(test_addr()).await.unwrap();

    // Discard start event
    let _ = rx.recv().await;

    manager.set_volume(-10.0).await;

    let event = rx.recv().await.unwrap();
    if let SessionEvent::VolumeChanged { volume, .. } = event {
        assert!((volume - -10.0).abs() < 0.001);
    } else {
        panic!("Expected VolumeChanged event");
    }
}
