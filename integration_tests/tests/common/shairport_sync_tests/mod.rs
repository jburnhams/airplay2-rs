use std::fs;
use std::path::PathBuf;

use crate::common::shairport_sync::{
    OutputBackend, ShairportAudioFormat, ShairportConfig, ShairportSync,
};

#[tokio::test]
async fn test_config_generation_basic() {
    let config = ShairportConfig {
        name: "test-receiver-1".to_string(),
        port: 5000,
        password: None,
        pipe_path: PathBuf::from("/tmp/test-pipe-1"),
        metadata_pipe_path: None,
        output_backend: OutputBackend::Pipe,
        audio_format: ShairportAudioFormat::S16LE,
        airplay2_enabled: true,
        interface: Some("lo".to_string()),
        log_verbosity: 2,
        udp_port_base: 6000,
    };

    let path = config
        .generate_config_file()
        .expect("Failed to generate config");
    assert!(path.exists());

    let content = fs::read_to_string(&path).expect("Failed to read config file");
    assert!(content.contains("name = \"test-receiver-1\";"));
    assert!(content.contains("port = 5000;"));
    assert!(content.contains("output_backend = \"pipe\";"));
    assert!(content.contains("interface = \"lo\";"));
    assert!(content.contains("name = \"/tmp/test-pipe-1\";"));

    // Cleanup
    let _ = fs::remove_file(path);
}

#[tokio::test]
async fn test_config_generation_with_password() {
    let config = ShairportConfig {
        name: "test-receiver-2".to_string(),
        port: 5001,
        password: Some("secret123".to_string()),
        pipe_path: PathBuf::from("/tmp/test-pipe-2"),
        metadata_pipe_path: None,
        output_backend: OutputBackend::Stdout,
        audio_format: ShairportAudioFormat::S16LE,
        airplay2_enabled: false,
        interface: None,
        log_verbosity: 1,
        udp_port_base: 6000,
    };

    let path = config
        .generate_config_file()
        .expect("Failed to generate config");
    let content = fs::read_to_string(&path).expect("Failed to read config file");
    assert!(content.contains("password = \"secret123\";"));

    // Cleanup
    let _ = fs::remove_file(path);
}

#[tokio::test]
async fn test_shairport_device_config() {
    let config = ShairportConfig {
        name: "test-device-cfg".to_string(),
        port: 5002,
        password: None,
        pipe_path: PathBuf::from("/tmp/test-pipe-3"),
        metadata_pipe_path: None,
        output_backend: OutputBackend::Pipe,
        audio_format: ShairportAudioFormat::S16LE,
        airplay2_enabled: true,
        interface: None,
        log_verbosity: 0,
        udp_port_base: 6000,
    };

    let device = ShairportSync::build_device_config(&config);
    assert_eq!(device.name, "test-device-cfg");
    assert_eq!(device.port, 5002);
    assert!(device.capabilities.airplay2);
}

#[tokio::test]
#[ignore = "Requires shairport-sync binary"]
async fn test_shairport_start_stop() {
    let config = ShairportConfig {
        name: "test-start-stop".to_string(),
        port: 5003,
        password: None,
        pipe_path: PathBuf::from("/tmp/test-pipe-start-stop"),
        metadata_pipe_path: None,
        output_backend: OutputBackend::Pipe,
        audio_format: ShairportAudioFormat::S16LE,
        airplay2_enabled: true,
        interface: None,
        log_verbosity: 3,
        udp_port_base: 6000,
    };

    let shairport = ShairportSync::start(config)
        .await
        .expect("Failed to start shairport-sync");
    let output = shairport
        .stop()
        .await
        .expect("Failed to stop shairport-sync");

    // Process should have exited cleanly (or killed)
    assert!(output.exit_status.is_some());
}
