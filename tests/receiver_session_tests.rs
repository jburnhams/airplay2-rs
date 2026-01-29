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
    let max_retries = 5;
    let mut retry_count = 0;

    loop {
        let config = SessionManagerConfig {
            udp_base_port: random_base_port(),
            ..Default::default()
        };
        let manager = SessionManager::new(config);
        let mut events = manager.subscribe();

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 5000);

        // Start session
        if let Err(e) = manager.start_session(addr).await {
            if retry_count < max_retries {
                eprintln!("Start session failed: {:?}, retrying...", e);
                retry_count += 1;
                continue;
            }
            panic!(
                "Failed to start session after {} retries: {:?}",
                max_retries, e
            );
        }

        // Verify start event
        let event = events.recv().await.unwrap();
        assert!(matches!(event, SessionEvent::SessionStarted { .. }));

        // Allocate sockets - this is where binding usually fails
        match manager.allocate_sockets().await {
            Ok((audio, control, timing)) => {
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
                break; // Success
            }
            Err(e) => {
                // If binding failed, retry with new port
                if retry_count < max_retries {
                    eprintln!("Socket allocation failed: {:?}, retrying...", e);
                    retry_count += 1;
                    continue;
                }
                panic!(
                    "Failed to allocate sockets after {} retries: {:?}",
                    max_retries, e
                );
            }
        }
    }
}

#[tokio::test]
async fn test_session_preemption() {
    let mut retry_count = 0;
    let max_retries = 5;

    loop {
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
        if let Err(e) = manager.start_session(addr1).await {
            if retry_count < max_retries {
                eprintln!("Preemption test start failed: {:?}, retrying...", e);
                retry_count += 1;
                continue;
            }
            panic!(
                "Preemption test failed to start after {} retries: {:?}",
                max_retries, e
            );
        }
        let _ = events.recv().await; // SessionStarted

        // Preempt with second session
        if let Err(e) = manager.start_session(addr2).await {
            if retry_count < max_retries {
                eprintln!("Preemption test 2nd start failed: {:?}, retrying...", e);
                retry_count += 1;
                continue;
            }
            panic!(
                "Preemption test 2nd failed to start after {} retries: {:?}",
                max_retries, e
            );
        }

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
        break;
    }
}

#[tokio::test]
async fn test_session_timeout() {
    let mut retry_count = 0;
    let max_retries = 5;

    loop {
        let config = SessionManagerConfig {
            idle_timeout: Duration::from_millis(100),
            udp_base_port: random_base_port(),
            ..Default::default()
        };

        let manager = std::sync::Arc::new(SessionManager::new(config));
        let mut events = manager.subscribe();

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 1000);

        if let Err(e) = manager.start_session(addr).await {
            if retry_count < max_retries {
                eprintln!("Timeout test start failed: {:?}, retrying...", e);
                retry_count += 1;
                continue;
            }
            panic!(
                "Timeout test failed to start after {} retries: {:?}",
                max_retries, e
            );
        }

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
        break;
    }
}
