use super::transport::{CastMode, LowerTransport, TransportHeader};

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
