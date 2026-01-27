use airplay2::receiver::session::SessionState;
use airplay2::receiver::session_manager::{
    PreemptionPolicy, SessionEvent, SessionManager, SessionManagerConfig,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

#[tokio::test]
async fn test_complete_session_lifecycle() {
    let manager = SessionManager::new(SessionManagerConfig::default());
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
    let _ = events.recv().await.unwrap(); // StateChanged Setup

    manager.update_state(SessionState::Streaming).await.unwrap();
    let _ = events.recv().await.unwrap(); // StateChanged Streaming

    // Set volume
    manager.set_volume(-20.0).await;
    let event = events.recv().await.unwrap();
    assert!(
        matches!(event, SessionEvent::VolumeChanged { volume, .. } if (volume - -20.0).abs() < 0.01)
    );

    // End session
    manager.end_session("Test complete").await;
    let event = events.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::SessionEnded { .. }));

    assert!(!manager.has_active_session().await);
}

#[tokio::test]
async fn test_session_preemption() {
    let manager = SessionManager::new(SessionManagerConfig {
        preemption_policy: PreemptionPolicy::AllowPreempt,
        ..Default::default()
    });

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
    assert!(matches!(
        event,
        SessionEvent::SessionEnded { reason, .. }
        if reason.contains("Preempted")
    ));

    let event = events.recv().await.unwrap();
    assert!(matches!(
        event,
        SessionEvent::SessionStarted { client, .. }
        if client == addr2
    ));
}

#[tokio::test]
async fn test_session_timeout() {
    let config = SessionManagerConfig {
        idle_timeout: Duration::from_millis(100),
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

    assert!(matches!(
        event,
        SessionEvent::SessionEnded { reason, .. }
        if reason.contains("Idle timeout")
    ));
}
