#[cfg(test)]
use crate::connection::{ConnectionManager, ConnectionState, ConnectionStats};

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
    use super::*;

    #[test]
    fn test_transport_parsing_valid() {
        let header =
            "RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002";
        let (audio, ctrl, time) = ConnectionManager::parse_transport_ports(header).unwrap();

        assert_eq!(audio, 6000);
        assert_eq!(ctrl, 6001);
        assert_eq!(time, 6002);
    }

    #[test]
    fn test_transport_parsing_whitespace() {
        let header = "RTP/AVP/UDP; unicast; mode=record; server_port=6000; control_port=6001; timing_port=6002";
        let (audio, ctrl, time) = ConnectionManager::parse_transport_ports(header).unwrap();

        assert_eq!(audio, 6000);
        assert_eq!(ctrl, 6001);
        assert_eq!(time, 6002);
    }

    #[test]
    fn test_transport_parsing_reordered() {
        let header = "control_port=6001;server_port=6000;timing_port=6002";
        let (audio, ctrl, time) = ConnectionManager::parse_transport_ports(header).unwrap();

        assert_eq!(audio, 6000);
        assert_eq!(ctrl, 6001);
        assert_eq!(time, 6002);
    }

    #[test]
    fn test_transport_parsing_extra_fields() {
        let header = "server_port=6000;foo=bar;control_port=6001;timing_port=6002;baz=qux";
        let (audio, ctrl, time) = ConnectionManager::parse_transport_ports(header).unwrap();

        assert_eq!(audio, 6000);
        assert_eq!(ctrl, 6001);
        assert_eq!(time, 6002);
    }

    #[test]
    fn test_transport_parsing_missing_audio() {
        // Missing server_port should fail
        let header = "control_port=6001;timing_port=6002";
        let result = ConnectionManager::parse_transport_ports(header);
        assert!(result.is_err());
    }

    #[test]
    fn test_transport_parsing_invalid_values() {
        // Non-numeric port
        let header = "server_port=abc;control_port=6001;timing_port=6002";
        let result = ConnectionManager::parse_transport_ports(header);
        assert!(result.is_err());
    }

    #[test]
    fn test_transport_parsing_partial_defaults() {
        // If control or timing ports are missing, they default to 0
        let header = "server_port=6000";
        let (audio, ctrl, time) = ConnectionManager::parse_transport_ports(header).unwrap();

        assert_eq!(audio, 6000);
        assert_eq!(ctrl, 0);
        assert_eq!(time, 0);
    }
}
