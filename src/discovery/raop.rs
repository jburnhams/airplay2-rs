//! RAOP (AirPlay 1) service discovery logic

/// RAOP service type for mDNS discovery
pub const RAOP_SERVICE_TYPE: &str = "_raop._tcp.local.";

/// Parse RAOP service instance name
///
/// RAOP service names follow the format: `{MAC_ADDRESS}@{DEVICE_NAME}`
/// Example: "0050C212A23F@Living Room"
pub fn parse_raop_service_name(name: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = name.splitn(2, '@').collect();
    if parts.len() == 2 {
        let mac = parts[0].to_uppercase();
        let device_name = parts[1].to_string();

        // Validate MAC address format (12 hex characters)
        if mac.len() == 12 && mac.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some((mac, device_name));
        }
    }
    None
}

/// Format MAC address with colons
pub fn format_mac_address(mac: &str) -> String {
    mac.chars()
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
