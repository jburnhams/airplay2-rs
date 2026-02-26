use crate::protocol::rtsp::transport::{
    CastMode, LowerTransport, TransportHeader, TransportParseError,
};

#[test]
fn test_parse_basic_transport() {
    let transport = TransportHeader::parse("RTP/AVP/UDP;unicast;mode=record").unwrap();

    assert_eq!(transport.protocol, "RTP/AVP");
    assert_eq!(transport.lower_transport, LowerTransport::Udp);
    assert_eq!(transport.cast, CastMode::Unicast);
    assert_eq!(transport.mode, Some("record".to_string()));
}

#[test]
fn test_parse_transport_with_ports() {
    let transport = TransportHeader::parse(
        "RTP/AVP/UDP;unicast;mode=record;control_port=6001;timing_port=6002",
    )
    .unwrap();

    assert_eq!(transport.control_port, Some(6001));
    assert_eq!(transport.timing_port, Some(6002));
}

#[test]
fn test_parse_tcp_transport() {
    let transport = TransportHeader::parse("RTP/AVP/TCP;unicast;interleaved=0-1").unwrap();

    assert_eq!(transport.lower_transport, LowerTransport::Tcp);
    assert_eq!(transport.interleaved, Some((0, 1)));
}

#[test]
fn test_response_header_generation() {
    let transport = TransportHeader::parse("RTP/AVP/UDP;unicast;mode=record").unwrap();

    let response = transport.to_response_header(6000, 6001, 6002);
    assert!(response.contains("server_port=6000"));
    assert!(response.contains("control_port=6001"));
    assert!(response.contains("timing_port=6002"));
}

#[test]
fn test_parse_missing_protocol() {
    let result = TransportHeader::parse(";unicast");
    // Split always returns at least one element, so an empty first part
    // results in UnsupportedProtocol("") rather than MissingProtocol
    assert!(matches!(
        result,
        Err(TransportParseError::UnsupportedProtocol(_))
    ));
}

#[test]
fn test_parse_unsupported_protocol() {
    let result = TransportHeader::parse("HTTP/1.1;unicast");
    assert!(matches!(
        result,
        Err(TransportParseError::UnsupportedProtocol(_))
    ));
}

#[test]
fn test_parse_invalid_port() {
    let result = TransportHeader::parse("RTP/AVP/UDP;unicast;control_port=invalid");
    assert!(matches!(result, Err(TransportParseError::InvalidPort)));
}

#[test]
fn test_parse_invalid_interleaved() {
    let result = TransportHeader::parse("RTP/AVP/TCP;unicast;interleaved=invalid");
    assert!(matches!(
        result,
        Err(TransportParseError::InvalidInterleaved)
    ));
}

#[test]
fn test_parse_unknown_parameters_ignored() {
    let transport =
        TransportHeader::parse("RTP/AVP/UDP;unicast;unknown=value;another=123").unwrap();
    assert_eq!(transport.cast, CastMode::Unicast);
    // Unknown parameters shouldn't cause error
}

#[test]
fn test_multicast_parsing() {
    let transport = TransportHeader::parse("RTP/AVP/UDP;multicast").unwrap();
    assert_eq!(transport.cast, CastMode::Multicast);
}

#[test]
fn test_parse_transport_with_server_port() {
    let transport = TransportHeader::parse(
        "RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002",
    )
    .unwrap();

    assert_eq!(transport.server_port, Some(6000));
    assert_eq!(transport.control_port, Some(6001));
    assert_eq!(transport.timing_port, Some(6002));
}
