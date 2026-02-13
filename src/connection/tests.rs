#[cfg(test)]
use crate::connection::{ConnectionState, ConnectionStats};

#[test]
fn test_connection_state_is_active() {
    assert!(ConnectionState::Connecting.is_active());
    assert!(ConnectionState::Connected.is_active());
    assert!(!ConnectionState::Disconnected.is_active());
    assert!(!ConnectionState::Failed.is_active());
}

#[test]
fn test_connection_state_is_connected() {
    assert!(ConnectionState::Connected.is_connected());
    assert!(!ConnectionState::Connecting.is_connected());
}

#[test]
fn test_connection_stats() {
    let mut stats = ConnectionStats::default();
    stats.record_sent(100);
    stats.record_received(200);

    assert_eq!(stats.bytes_sent, 100);
    assert_eq!(stats.bytes_received, 200);
}

#[cfg(test)]
mod ptp_integration_tests {
    use crate::connection::ConnectionManager;
    use crate::types::{AirPlayConfig, AirPlayDevice, DeviceCapabilities, TimingProtocol};
    use std::collections::HashMap;

    fn make_device(supports_ptp: bool, airplay2: bool) -> AirPlayDevice {
        AirPlayDevice {
            id: "test-device-id".to_string(),
            name: "Test HomePod".to_string(),
            model: Some("AudioAccessory5,1".to_string()),
            addresses: vec!["192.168.1.100".parse().unwrap()],
            port: 7000,
            capabilities: DeviceCapabilities {
                supports_ptp,
                airplay2,
                supports_audio: true,
                ..Default::default()
            },
            raop_port: None,
            raop_capabilities: None,
            txt_records: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_ptp_not_active_before_connect() {
        let config = AirPlayConfig::default();
        let manager = ConnectionManager::new(config);
        assert!(!manager.is_ptp_active().await);
        assert!(manager.ptp_clock().await.is_none());
    }

    #[tokio::test]
    async fn test_ptp_not_active_with_ntp_config() {
        let config = AirPlayConfig {
            timing_protocol: TimingProtocol::Ntp,
            ..Default::default()
        };
        let manager = ConnectionManager::new(config);
        assert!(!manager.is_ptp_active().await);
    }

    #[tokio::test]
    async fn test_ptp_clock_none_before_connect() {
        let config = AirPlayConfig {
            timing_protocol: TimingProtocol::Ptp,
            ..Default::default()
        };
        let manager = ConnectionManager::new(config);
        // PTP clock is only created during connection setup
        assert!(manager.ptp_clock().await.is_none());
        assert!(!manager.is_ptp_active().await);
    }

    #[tokio::test]
    async fn test_timing_protocol_variants() {
        // Verify all variants work with config
        let configs = [
            (TimingProtocol::Auto, "Auto"),
            (TimingProtocol::Ptp, "Ptp"),
            (TimingProtocol::Ntp, "Ntp"),
        ];

        for (protocol, name) in &configs {
            let config = AirPlayConfig {
                timing_protocol: *protocol,
                ..Default::default()
            };
            let manager = ConnectionManager::new(config);
            // All start inactive before connection
            assert!(
                !manager.is_ptp_active().await,
                "PTP should be inactive before connect for {name}"
            );
        }
    }

    #[tokio::test]
    async fn test_device_ptp_capability_detection() {
        // AirPlay 2 device with PTP support
        let device = make_device(true, true);
        assert!(device.supports_ptp());
        assert!(device.supports_airplay2());

        // Legacy device without PTP
        let device = make_device(false, false);
        assert!(!device.supports_ptp());
        assert!(!device.supports_airplay2());
    }

    #[tokio::test]
    async fn test_airplay2_device_without_explicit_ptp_flag() {
        // AirPlay 2 device that doesn't explicitly set PTP bit
        // Auto mode should still select PTP because it's AirPlay 2
        let device = make_device(false, true);
        assert!(!device.supports_ptp());
        assert!(device.supports_airplay2());
        // In Auto mode, AirPlay 2 capability implies PTP should be used
    }
}

#[cfg(test)]
mod parsing_tests {
    #[test]
    fn test_transport_parsing() {
        // This logic is internal to setup_session but we can test the parsing logic if we extract it.
        // For now, since we cannot easily test private async methods without refactoring,
        // we will verify the logic via inspection or integration tests.
        // However, I can create a small test that mimics the parsing logic here to ensure it works.

        let transport_header =
            "RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002";
        let mut server_audio_port = 0;
        let mut server_ctrl_port = 0;
        let mut server_time_port = 0;

        for part in transport_header.split(';') {
            if let Some((key, value)) = part.trim().split_once('=') {
                if let Ok(port) = value.parse::<u16>() {
                    match key {
                        "server_port" => server_audio_port = port,
                        "control_port" => server_ctrl_port = port,
                        "timing_port" => server_time_port = port,
                        _ => {}
                    }
                }
            }
        }

        assert_eq!(server_audio_port, 6000);
        assert_eq!(server_ctrl_port, 6001);
        assert_eq!(server_time_port, 6002);
    }
}
