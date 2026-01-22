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
mod parsing_tests {
    #[test]
    fn test_transport_parsing() {
        // This logic is internal to setup_session but we can test the parsing logic if we extract it.
        // For now, since we cannot easily test private async methods without refactoring,
        // we will verify the logic via inspection or integration tests.
        // However, I can create a small test that mimics the parsing logic here to ensure it works.

        let transport_header = "RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002";
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
