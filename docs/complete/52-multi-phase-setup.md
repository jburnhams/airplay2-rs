# Section 52: Multi-Phase SETUP Handler

## Dependencies
- **Section 46**: AirPlay 2 Receiver Overview
- **Section 48**: RTSP/HTTP Server Extensions
- **Section 53**: Encrypted Control Channel (for decrypting SETUP bodies)
- **Section 03**: Binary Plist Codec

## Overview

AirPlay 2 uses a two-phase SETUP process, unlike AirPlay 1's single SETUP. This allows for more complex channel negotiation including event, timing, and multiple audio channels.

### SETUP Phases

**Phase 1: Event and Timing Channels**
- Establishes the event channel for async notifications
- Sets up timing synchronization (PTP or NTP)
- Allocates UDP ports for timing packets

**Phase 2: Audio Streams**
- Configures audio format and encryption
- Allocates UDP ports for audio data and control
- Sets up buffering parameters

```
Sender                              Receiver
  │                                    │
  │─── SETUP (phase 1) ───────────────▶│
  │    streams: [eventChannel, timing] │
  │                                    │
  │◀── Response ──────────────────────│
  │    eventPort: 7010                 │
  │    timingPort: 7011                │
  │                                    │
  │─── SETUP (phase 2) ───────────────▶│
  │    streams: [audioData, control]   │
  │    Audio format, encryption params │
  │                                    │
  │◀── Response ──────────────────────│
  │    dataPort: 7100                  │
  │    controlPort: 7101               │
  │    audioLatency: 88200             │
```

## Objectives

- Parse two-phase SETUP requests (binary plist bodies)
- Allocate UDP ports for each channel
- Configure audio streaming parameters
- Manage stream state across phases
- Support both encrypted and unencrypted SETUP bodies

---

## Tasks

### 52.1 SETUP Request Parsing

- [x] **52.1.1** Define SETUP request structures

**File:** `src/receiver/ap2/setup_handler.rs`

### 52.2 SETUP Response Builder

- [x] **52.2.1** Build SETUP response with allocated ports

**File:** `src/receiver/ap2/setup_handler.rs` (continued)

### 52.3 SETUP Handler Implementation

- [x] **52.3.1** Implement SETUP request handler

**File:** `src/receiver/ap2/setup_handler.rs` (continued)

---

## Unit Tests

### 52.4 SETUP Tests

- [x] **52.4.1** Test SETUP request parsing and response generation

**File:** `src/receiver/ap2/setup_handler.rs` (test module)

---

## Acceptance Criteria

- [x] Phase 1 SETUP correctly parses event/timing streams
- [x] Phase 2 SETUP correctly parses audio streams
- [x] Port allocation works within configured range
- [x] Response contains all required fields
- [x] Audio latency correctly reported
- [x] Session state advances through phases
- [x] Events emitted for pipeline coordination
- [x] Cleanup releases all allocated ports
- [x] All unit tests pass

---

## Notes

### Encrypted SETUP Bodies

After pairing, SETUP request bodies are encrypted with the session key. The handler
should work with the encrypted channel layer (Section 53) which handles decryption
before the body reaches this handler.

### Timing Protocol Selection

- **PTP**: Preferred for AirPlay 2, enables multi-room sync
- **NTP**: Fallback for compatibility
- The receiver should support both and use what the sender requests

---

## References

- [AirPlay 2 SETUP Analysis](https://emanuelecozzi.net/docs/airplay2/rtsp/)
- [Section 48: RTSP/HTTP Server](./48-rtsp-http-server-extensions.md)
