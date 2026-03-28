use std::time::Duration;

use crate::common::subprocess::{ReadyStrategy, SubprocessConfig, SubprocessHandle};

mod common;

#[tokio::test]
async fn test_subprocess_config_defaults() {
    let config = SubprocessConfig::default();
    assert_eq!(config.command, "");
    assert!(config.args.is_empty());
    assert!(config.working_dir.is_none());
    assert!(config.env_vars.is_empty());
    assert_eq!(config.ready_timeout, Duration::from_secs(15));
    assert_eq!(config.shutdown_timeout, Duration::from_secs(5));
    assert_eq!(config.log_prefix, "[subprocess]");
    assert!(config.post_ready_delay.is_none());
    assert_eq!(config.max_log_lines, 10000);
}

#[tokio::test]
async fn test_subprocess_spawn_missing_executable() {
    let config = SubprocessConfig {
        command: "non_existent_executable_12345".to_string(),
        ready_strategy: ReadyStrategy::Delay(Duration::from_millis(10)),
        ..Default::default()
    };

    let res = SubprocessHandle::spawn(config).await;
    assert!(res.is_err());
    let err = match res {
        Err(e) => e,
        Ok(_) => panic!("Expected error"),
    };
    match err {
        crate::common::subprocess::SubprocessError::SpawnFailed { command, source } => {
            assert_eq!(command, "non_existent_executable_12345");
            assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        }
        _ => panic!("Expected SpawnFailed"),
    }
}

#[tokio::test]
async fn test_subprocess_custom_ready_strategy() {
    let config = SubprocessConfig {
        command: "sleep".to_string(),
        args: vec!["2".to_string()],
        ready_strategy: ReadyStrategy::Custom(Box::new(|| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                true
            })
        })),
        ready_timeout: Duration::from_secs(1),
        ..Default::default()
    };

    let handle = SubprocessHandle::spawn(config)
        .await
        .expect("Failed to spawn process");
    handle.stop().await.expect("Failed to stop process");
}

#[tokio::test]
async fn test_subprocess_custom_ready_strategy_timeout() {
    let config = SubprocessConfig {
        command: "sleep".to_string(),
        args: vec!["2".to_string()],
        ready_strategy: ReadyStrategy::Custom(Box::new(|| {
            Box::pin(async {
                false // Never ready
            })
        })),
        ready_timeout: Duration::from_millis(100),
        ..Default::default()
    };

    let res = SubprocessHandle::spawn(config).await;
    assert!(res.is_err());
    let err = match res {
        Err(e) => e,
        Ok(_) => panic!("Expected error"),
    };
    match err {
        crate::common::subprocess::SubprocessError::ReadyTimeout { timeout, .. } => {
            assert_eq!(timeout, Duration::from_millis(100));
        }
        _ => panic!("Expected ReadyTimeout"),
    }
}

#[tokio::test]
async fn test_subprocess_log_truncation() {
    let config = SubprocessConfig {
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "for i in $(seq 1 10); do echo \"line $i\"; done && sleep 1".to_string(),
        ],
        ready_strategy: ReadyStrategy::Delay(Duration::from_millis(500)),
        max_log_lines: 5,
        ..Default::default()
    };

    let handle = SubprocessHandle::spawn(config)
        .await
        .expect("Failed to spawn process");
    let output = handle.stop().await.expect("Failed to stop process");

    // Process should run and echo 10 lines, but max_log_lines is 5
    // Wait, the task might capture 5 and then ignore the rest
    // The delay makes sure it has time to run
    assert!(output.logs.len() <= 5);
}
