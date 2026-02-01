use crate::discovery::raop::*;

#[test]
fn test_parse_raop_service_name() {
    let (mac, name) = parse_raop_service_name("0050C212A23F@Living Room").unwrap();
    assert_eq!(mac, "0050C212A23F");
    assert_eq!(name, "Living Room");
}

#[test]
fn test_parse_raop_service_name_with_special_chars() {
    let (mac, name) = parse_raop_service_name("AABBCCDDEEFF@Speaker's Room").unwrap();
    assert_eq!(mac, "AABBCCDDEEFF");
    assert_eq!(name, "Speaker's Room");
}

#[test]
fn test_format_mac_address() {
    assert_eq!(format_mac_address("0050C212A23F"), "00:50:C2:12:A2:3F");
}
