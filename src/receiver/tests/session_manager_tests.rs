use super::super::session::SessionError;
use super::super::session_manager::{PreemptionPolicy, SessionManager, SessionManagerConfig};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[tokio::test]
async fn test_session_manager_single_session() {
    let manager = SessionManager::new(SessionManagerConfig::default());

    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
    let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 1001);

    // Start first session
    let session1 = manager.start_session(addr1).await.unwrap();
    assert!(manager.has_active_session().await);

    // With AllowPreempt policy, second session preempts first
    let session2 = manager.start_session(addr2).await.unwrap();
    assert_ne!(session1, session2);

    // End session
    manager.end_session("test").await;
    assert!(!manager.has_active_session().await);
}

#[tokio::test]
async fn test_session_manager_reject_policy() {
    let config = SessionManagerConfig {
        preemption_policy: PreemptionPolicy::Reject,
        ..Default::default()
    };
    let manager = SessionManager::new(config);

    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
    let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 1001);

    manager.start_session(addr1).await.unwrap();

    // Second session should be rejected
    let result = manager.start_session(addr2).await;
    assert!(matches!(result, Err(SessionError::Busy)));
}

#[tokio::test]
async fn test_socket_allocation() {
    let manager = SessionManager::new(SessionManagerConfig::default());

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
    manager.start_session(addr).await.unwrap();

    let (audio, control, timing) = manager.allocate_sockets().await.unwrap();

    // Ports should be allocated
    assert!(audio > 0);
    assert!(control > 0);
    assert!(timing > 0);

    // Ports should be sequential (based on allocator)
    assert_eq!(control, audio + 1);
    assert_eq!(timing, audio + 2);
}
