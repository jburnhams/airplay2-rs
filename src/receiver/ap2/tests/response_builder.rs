use std::collections::HashMap;

use crate::protocol::plist::PlistValue;
use crate::receiver::ap2::response_builder::Ap2ResponseBuilder;

#[test]
fn test_bplist_response() {
    let mut dict = HashMap::new();
    dict.insert("status".to_string(), PlistValue::Integer(0));
    dict.insert("message".to_string(), PlistValue::String("OK".to_string()));

    let plist = PlistValue::Dictionary(dict);

    let response = Ap2ResponseBuilder::ok()
        .cseq(5)
        .bplist_body(&plist)
        .unwrap()
        .encode();

    let response_str = String::from_utf8_lossy(&response);
    assert!(response_str.contains("200 OK"));
    assert!(response_str.contains("Content-Type: application/x-apple-binary-plist"));
}

#[test]
fn test_timing_response() {
    let response = Ap2ResponseBuilder::ok()
        .cseq(10)
        .timing_port(7011)
        .event_port(7012)
        .encode();

    let response_str = String::from_utf8_lossy(&response);
    assert!(response_str.contains("Timing-Port: 7011"));
    assert!(response_str.contains("Event-Port: 7012"));
}
