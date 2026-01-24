# Section 25: RAOP Service Discovery

## Dependencies
- **Section 08**: mDNS Discovery (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)
- **Section 24**: AirPlay 1 Overview (should be reviewed)

## Overview

AirPlay 1 devices (AirPort Express, older Apple TVs, third-party receivers) advertise themselves via the `_raop._tcp` mDNS service type, distinct from AirPlay 2's `_airplay._tcp`. This section extends the existing mDNS discovery infrastructure to detect and parse RAOP service advertisements.

## Objectives

- Extend service browser to discover `_raop._tcp` services
- Parse RAOP-specific TXT records
- Detect device capabilities (codecs, encryption types)
- Distinguish between AirPlay 1-only and dual-protocol devices
- Integrate with existing `AirPlayDevice` type

---

## Tasks

### 25.1 RAOP Service Types

- [x] **25.1.1** Define RAOP service constants and types

**File:** `src/discovery/raop.rs`

```rust
//! RAOP (AirPlay 1) service discovery

/// RAOP service type for mDNS discovery
pub const RAOP_SERVICE_TYPE: &str = "_raop._tcp.local.";
// ...
```

### 25.2 RAOP Capabilities Parsing

- [x] **25.2.1** Implement RAOP TXT record parser

**File:** `src/discovery/raop.rs` (continued)

```rust
/// RAOP device capabilities parsed from TXT records
#[derive(Debug, Clone, Default)]
pub struct RaopCapabilities {
// ...
```

### 25.3 RAOP Service Browser

- [x] **25.3.1** Extend discovery browser for RAOP services

**File:** `src/discovery/browser.rs` (extensions)

```rust
use super::raop::{RaopCapabilities, RAOP_SERVICE_TYPE};
// ...
```

### 25.4 RAOP Service Name Parsing

- [x] **25.4.1** Parse RAOP service instance names

**File:** `src/discovery/raop.rs` (continued)

```rust
/// Parse RAOP service instance name
pub fn parse_raop_service_name(name: &str) -> Option<(String, String)> {
// ...
```

### 25.5 Unified Discovery API

- [x] **25.5.1** Implement unified discovery stream

**File:** `src/discovery/mod.rs` (extensions)

```rust
/// Start continuous discovery for both AirPlay 1 and 2 devices
pub async fn discover_all(options: DiscoveryOptions) -> impl Stream<Item = DiscoveryEvent> {
// ...
```

---

## Unit Tests

### Test File: `src/discovery/raop.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_capabilities_basic() {
// ...
```

---

## Integration Tests

### Test: Discovery of RAOP devices

```rust
// tests/discovery_raop_integration.rs

use airplay2_rs::discovery::{discover_all, DiscoveryOptions, DiscoveryEvent, DeviceProtocol};
// ...
```

---

## Acceptance Criteria

- [x] RAOP service type is correctly browsed via mDNS
- [x] All TXT record fields are parsed correctly
- [x] Codec list parsing handles all valid formats
- [x] Encryption type detection is accurate
- [x] Service name parsing extracts MAC and device name
- [x] Device protocol detection distinguishes AirPlay 1/2/Both
- [x] Unified discovery API returns correlated devices
- [x] Password-protected devices are detected
- [x] Missing TXT fields use sensible defaults
- [x] All unit tests pass
- [x] Integration tests with mock services pass

---

## Notes

- Some older devices may have non-standard TXT record formats
- MAC address in service name may not match actual network interface
- Consider caching discovered devices for quick reconnection
- Network changes should trigger re-discovery
- Some devices advertise both services with different capabilities
