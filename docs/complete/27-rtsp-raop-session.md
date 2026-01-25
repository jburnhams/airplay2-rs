# Section 27: RTSP Session for RAOP

## Dependencies
- **Section 05**: RTSP Protocol (must be complete)
- **Section 26**: RSA Authentication (must be complete)
- **Section 25**: RAOP Discovery (recommended)

## Overview

AirPlay 1 uses RTSP (Real Time Streaming Protocol) for session control, similar to AirPlay 2 but with significant differences:

- **SDP bodies** instead of binary plist for stream configuration
- **Apple-Challenge/Response headers** for device authentication
- **Different method sequence** (OPTIONS → ANNOUNCE → SETUP → RECORD)
- **Legacy headers** (CSeq, Session, RTP-Info)

This section extends the existing RTSP codec to support RAOP-specific requirements.

## Objectives

- Implement SDP (Session Description Protocol) parsing and generation
- Add RAOP-specific RTSP headers
- Implement RAOP session state machine
- Support all RAOP RTSP methods
- Handle audio format negotiation via SDP

---

## Tasks

### 27.1 SDP Protocol Implementation

- [x] **27.1.1** Define SDP types and parser

**File:** `src/protocol/sdp/mod.rs`

- [x] **27.1.2** Implement SDP parser

**File:** `src/protocol/sdp/parser.rs`

- [x] **27.1.3** Implement SDP builder

**File:** `src/protocol/sdp/builder.rs`

---

### 27.2 RAOP RTSP Extensions

- [x] **27.2.1** Add RAOP-specific headers

**File:** `src/protocol/rtsp/headers.rs` (additions)

- [x] **27.2.2** Implement RAOP RTSP session

**File:** `src/protocol/raop/session.rs`

---

## Unit Tests

### Test File: `src/protocol/sdp/tests.rs`

See `src/protocol/sdp/tests.rs` for implementation.

### Test File: `src/protocol/raop/session_tests.rs`

See `src/protocol/raop/session_tests.rs` for implementation.

---

## Integration Tests

### Test: Full RAOP RTSP session flow

See `tests/raop_rtsp_integration.rs` for implementation.

---

## Acceptance Criteria

- [x] SDP parsing handles all RAOP fields correctly
- [x] SDP generation produces valid output
- [x] RAOP session state machine transitions correctly
- [x] All RAOP RTSP methods are implemented
- [x] Apple-Challenge header is included in OPTIONS
- [x] Transport header parsing extracts all ports
- [x] Volume control uses correct dB format
- [x] Session keys are generated and encoded correctly
- [x] All unit tests pass
- [x] Integration tests pass

---

## Notes

- SDP parser should be lenient with whitespace variations
- Some devices may have non-standard SDP attributes
- Transport response format may vary between implementations
- Consider adding support for AAC codec SDP format
- Debug logging should show full request/response for protocol debugging
