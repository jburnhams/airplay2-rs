use std::time::Duration;

use crate::common::ports::{reserve_port_range, reserve_ports};
use crate::common::subprocess::{ReadyStrategy, SubprocessConfig, SubprocessHandle};

mod common;

#[tokio::test]
async fn test_subprocess_spawn_and_stop() {
    let config = SubprocessConfig {
        command: "sleep".to_string(),
        args: vec!["30".to_string()],
        ready_strategy: ReadyStrategy::Delay(Duration::from_millis(10)),
        ..Default::default()
    };

    let mut handle = SubprocessHandle::spawn(config)
        .await
        .expect("Failed to spawn process");
    assert!(handle.is_running().await);
    let output = handle.stop().await.expect("Failed to stop process");
    assert!(output.exit_status.is_some());
}

#[tokio::test]
async fn test_subprocess_early_exit() {
    let config = SubprocessConfig {
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "exit 1".to_string()],
        ready_strategy: ReadyStrategy::Delay(Duration::from_secs(5)),
        ..Default::default()
    };

    let res = SubprocessHandle::spawn(config).await;
    assert!(res.is_err());
    let err = match res {
        Err(e) => e,
        Ok(_) => panic!("Expected error"),
    };
    match err {
        crate::common::subprocess::SubprocessError::EarlyExit { status, .. } => {
            assert!(!status.success());
        }
        _ => panic!("Expected EarlyExit"),
    }
}

#[tokio::test]
async fn test_subprocess_ready_detection() {
    let config = SubprocessConfig {
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "echo 'ready pattern' && sleep 30".to_string(),
        ],
        ready_strategy: ReadyStrategy::LogPattern("ready pattern".to_string()),
        ready_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let mut handle = SubprocessHandle::spawn(config)
        .await
        .expect("Failed to spawn process");
    assert!(handle.is_running().await);
    handle.stop().await.unwrap();
}

#[tokio::test]
async fn test_subprocess_ready_timeout() {
    let config = SubprocessConfig {
        command: "sleep".to_string(),
        args: vec!["30".to_string()],
        ready_strategy: ReadyStrategy::LogPattern("impossible pattern".to_string()),
        ready_timeout: Duration::from_secs(1),
        ..Default::default()
    };

    let res = SubprocessHandle::spawn(config).await;
    assert!(res.is_err());
    let err = match res {
        Err(e) => e,
        Ok(_) => panic!("Expected error"),
    };
    match err {
        crate::common::subprocess::SubprocessError::ReadyTimeout { .. } => {}
        _ => panic!("Expected ReadyTimeout"),
    }
}

#[test]
fn test_reserve_ports_no_collisions() {
    let reserved = reserve_ports(10).unwrap();
    let ports = reserved.ports.clone();
    assert_eq!(ports.len(), 10);
    let mut sorted = ports.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ports.len(), sorted.len());
}

#[test]
fn test_reserve_ports_consecutive() {
    let range = reserve_port_range(3).unwrap();
    assert_eq!(range.ports.len(), 3);
    assert_eq!(range.ports[1], range.ports[0] + 1);
    assert_eq!(range.ports[2], range.ports[1] + 1);
}
