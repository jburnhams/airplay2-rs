# Section 48: RTSP/HTTP Server Extensions for AirPlay 2

## Dependencies
- **Section 36**: RTSP Server (Sans-IO) - existing server codec
- **Section 46**: AirPlay 2 Receiver Overview
- **Section 03**: Binary Plist Codec (for body parsing)

## Overview

AirPlay 2 uses a hybrid RTSP/HTTP protocol where some endpoints behave like HTTP (POST to paths like `/pair-setup`) while others are traditional RTSP methods. This section extends our existing RTSP server codec to handle AirPlay 2-specific requests.

### Key Differences from AirPlay 1

| Aspect | AirPlay 1 (RAOP) | AirPlay 2 |
|--------|------------------|-----------|
| Body Format | SDP, text/parameters | Binary plist |
| Endpoints | RTSP methods only | RTSP + HTTP-style POST paths |
| Content-Type | application/sdp | application/x-apple-binary-plist |
| Authentication | In-band RSA | Separate pairing endpoints |

### AirPlay 2 Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/info` | GET | Query device capabilities |
| `/pair-setup` | POST | SRP pairing setup (M1-M4) |
| `/pair-verify` | POST | Session verification (M1-M4) |
| `/fp-setup` | POST | FairPlay setup (not implemented) |
| `/command` | POST | Playback commands |
| `/feedback` | POST | Feedback/status channel |
| `/audioMode` | POST | Audio mode configuration |
| Standard RTSP | Various | SETUP, RECORD, etc. |

## Objectives

- Extend RTSP server codec to handle HTTP-style endpoints
- Add binary plist body parsing for AirPlay 2 requests
- Implement request routing based on method and path
- Support both encrypted and plaintext request handling
- Maintain sans-IO design principles

---

## Tasks

### 48.1 Request Type Detection

- [x] **48.1.1** Implement request type classification

**File:** `src/receiver/ap2/request_router.rs`

### 48.2 Binary Plist Body Handler

- [x] **48.2.1** Implement binary plist body parsing and generation

**File:** `src/receiver/ap2/body_handler.rs`

### 48.3 Extended Response Builder

- [x] **48.3.1** Extend response builder for AirPlay 2

**File:** `src/receiver/ap2/response_builder.rs`

### 48.4 Request Handler Framework

- [x] **48.4.1** Implement unified request handler for AirPlay 2

**File:** `src/receiver/ap2/request_handler.rs`

## Unit Tests

### 48.5 Server Extension Tests

- [x] **48.5.1** Comprehensive tests for request routing and handling

**File:** `src/receiver/ap2/request_handler.rs` (test module)

---

## Acceptance Criteria

- [x] Request classification correctly identifies RTSP vs endpoint requests
- [x] Binary plist bodies parse and encode correctly
- [x] Response builder generates valid RTSP responses with bplist bodies
- [x] Request routing enforces state-based access control
- [x] Authentication requirements enforced for protected endpoints
- [x] OPTIONS returns correct Public header for AirPlay 2
- [x] Unknown endpoints return 404
- [x] All unit tests pass

---

## Notes

### Encrypted Request Handling

After pairing completes, all control channel traffic is encrypted using ChaCha20-Poly1305.
The request handler framework supports this via the `decrypt` function in the context, but
the actual encryption/decryption is handled by Section 53 (Encrypted Control Channel).

### Request Body Processing

Most AirPlay 2 endpoints expect binary plist bodies. The handler framework parses these
automatically when the Content-Type header indicates bplist. Text parameters are still
supported for backward compatibility with some SET_PARAMETER commands.

---

## References

- [AirPlay 2 Protocol Analysis](https://emanuelecozzi.net/docs/airplay2)
- [Section 36: RTSP Server Sans-IO](./complete/36-rtsp-server-sans-io.md)
- [Section 03: Binary Plist Codec](./complete/03-binary-plist-codec.md)
