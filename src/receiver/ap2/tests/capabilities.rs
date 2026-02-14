use crate::protocol::plist::PlistValue;
use crate::receiver::ap2::capabilities::DeviceCapabilities;

#[test]
fn test_capabilities_to_plist() {
    let caps = DeviceCapabilities::audio_receiver("AA:BB:CC:DD:EE:FF", "Test Speaker", [0u8; 32]);

    let plist = caps.to_plist();

    if let PlistValue::Dictionary(dict) = plist {
        assert!(dict.contains_key("deviceid"));
        assert!(dict.contains_key("name"));
        assert!(dict.contains_key("features"));
        assert!(dict.contains_key("audioFormats"));
        assert!(dict.contains_key("pk"));
    } else {
        panic!("Expected Dictionary");
    }
}

#[test]
fn test_pairing_identity_deterministic() {
    let caps1 = DeviceCapabilities::audio_receiver("AA:BB:CC:DD:EE:FF", "Test Speaker", [0u8; 32]);
    let caps2 = DeviceCapabilities::audio_receiver("AA:BB:CC:DD:EE:FF", "Test Speaker", [0u8; 32]);

    assert_eq!(caps1.pairing_identity, caps2.pairing_identity);
    assert_eq!(caps1.pairing_identity.len(), 36); // UUID format
}
