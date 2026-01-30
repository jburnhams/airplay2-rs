use airplay2::receiver::session::SessionState;
use airplay2::receiver::session_manager::{
    PreemptionPolicy, SessionEvent, SessionManager, SessionManagerConfig,
};
use rand::Rng;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

fn random_base_port() -> u16 {
    rand::thread_rng().gen_range(40000..60000)
}

#[tokio::test]
async fn test_complete_session_lifecycle() {
    let config = SessionManagerConfig {
        udp_base_port: random_base_port(),
        ..Default::default()
    };
    let manager = SessionManager::new(config);
    let mut events = manager.subscribe();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 5000);

    // Start session
    let _session_id = manager.start_session(addr).await.unwrap();

    // Verify start event
    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::SessionStarted { .. }));

    // Allocate sockets
    let (audio, control, timing) = manager.allocate_sockets().await.unwrap();
    assert!(audio > 0 && control > 0 && timing > 0);

    // Progress through states
    manager.update_state(SessionState::Announced).await.unwrap();
    let event = events.recv().await.unwrap();
    assert!(matches!(
        event,
        SessionEvent::StateChanged {
            new_state: SessionState::Announced,
            ..
        }
    ));

    manager.update_state(SessionState::Setup).await.unwrap();
    let event = events.recv().await.unwrap();
    assert!(matches!(
        event,
        SessionEvent::StateChanged {
            new_state: SessionState::Setup,
            ..
        }
    ));

    manager.update_state(SessionState::Streaming).await.unwrap();
    let event = events.recv().await.unwrap();
    assert!(matches!(
        event,
        SessionEvent::StateChanged {
            new_state: SessionState::Streaming,
            ..
        }
    ));

    // Set volume
    manager.set_volume(-20.0).await;
    let event = events.recv().await.unwrap();
    if let SessionEvent::VolumeChanged { volume, .. } = event {
        assert!((volume - -20.0).abs() < 0.01);
    } else {
        panic!("Expected VolumeChanged, got {:?}", event);
    }

    // End session
    manager.end_session("Test complete").await;
    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::SessionEnded { .. }));

    assert!(!manager.has_active_session().await);
}

#[tokio::test]
async fn test_session_preemption() {
    let config = SessionManagerConfig {
        preemption_policy: PreemptionPolicy::AllowPreempt,
        udp_base_port: random_base_port(),
        ..Default::default()
    };
    let manager = SessionManager::new(config);

    let mut events = manager.subscribe();

    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
    let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 1001);

    // Start first session
    manager.start_session(addr1).await.unwrap();
    let _ = events.recv().await; // SessionStarted

    // Preempt with second session
    manager.start_session(addr2).await.unwrap();

    // Should get SessionEnded for first, then SessionStarted for second
    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::SessionEnded { reason, .. }
        if reason.contains("Preempted")));

    let event = events.recv().await.unwrap();
    if let SessionEvent::SessionStarted { client, .. } = event {
        assert_eq!(client, addr2);
    } else {
        panic!("Expected SessionStarted, got {:?}", event);
    }
}

#[tokio::test]
async fn test_session_timeout() {
    let config = SessionManagerConfig {
        idle_timeout: Duration::from_millis(100),
        udp_base_port: random_base_port(),
        ..Default::default()
    };

    let manager = std::sync::Arc::new(SessionManager::new(config));
    let mut events = manager.subscribe();

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
    manager.start_session(addr).await.unwrap();
    let _ = events.recv().await; // SessionStarted

    // Start timeout monitor
    let _monitor = manager.start_timeout_monitor();

    // Wait for timeout
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Should receive timeout event
    let event = tokio::time::timeout(Duration::from_millis(100), events.recv())
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(event, SessionEvent::SessionEnded { reason, .. }
        if reason.contains("timeout")));
}

#[tokio::test]
async fn test_session_preemption_reject() {
    let config = SessionManagerConfig {
        preemption_policy: PreemptionPolicy::Reject,
        udp_base_port: random_base_port(),
        ..Default::default()
    };
    let manager = SessionManager::new(config);

    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
    let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 1001);

    // Start first session
    manager.start_session(addr1).await.unwrap();

    // Try to start second session - should be rejected
    let result = manager.start_session(addr2).await;
    assert!(matches!(
        result,
        Err(airplay2::receiver::session::SessionError::Busy)
    ));
}

#[tokio::test]
async fn test_invalid_state_transition() {
    let config = SessionManagerConfig {
        udp_base_port: random_base_port(),
        ..Default::default()
    };
    let manager = SessionManager::new(config);

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);
    manager.start_session(addr).await.unwrap();

    // Connected -> Streaming is invalid (must go through Announced/Setup)
    let result = manager.update_state(SessionState::Streaming).await;
    assert!(matches!(
        result,
        Err(airplay2::receiver::session::SessionError::InvalidTransition { .. })
    ));
}
