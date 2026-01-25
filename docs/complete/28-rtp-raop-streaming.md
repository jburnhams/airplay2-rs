# Section 28: RTP Audio Streaming for RAOP

## Dependencies
- **Section 06**: RTP/RAOP Protocol (must be complete)
- **Section 27**: RTSP Session for RAOP (must be complete)
- **Section 29**: RAOP Audio Encryption (recommended)

## Overview

RAOP uses RTP (Real-time Transport Protocol) for audio data transmission over UDP. Unlike standard RTP, RAOP includes Apple-specific extensions for synchronization, retransmission, and timing. Three UDP channels are used:

1. **Audio Channel**: Encrypted audio packets
2. **Control Channel**: Sync packets and retransmission requests
3. **Timing Channel**: NTP-style clock synchronization

## Objectives

- Extend RTP packet types for RAOP-specific formats
- Implement sync packet generation and parsing
- Implement timing protocol (NTP-style)
- Handle retransmission requests and responses
- Support packet loss detection and recovery

---

## Tasks

### 28.1 RAOP RTP Packet Types

- [x] **28.1.1** Define RAOP-specific RTP payload types

**File:** `src/protocol/rtp/raop.rs`

### 28.2 Timing Protocol

- [x] **28.2.1** Implement NTP-style timing exchange

**File:** `src/protocol/rtp/raop_timing.rs`

### 28.3 Audio Packet Buffer

- [x] **28.3.1** Implement packet buffer for retransmission

**File:** `src/protocol/rtp/packet_buffer.rs`

### 28.4 RAOP Audio Streamer

- [x] **28.4.1** Implement audio streaming coordinator

**File:** `src/streaming/raop_streamer.rs`

---

## Acceptance Criteria

- [x] RAOP audio packets encode/decode correctly
- [x] Sync packets include correct timestamps
- [x] Timing protocol calculates offset accurately
- [x] Packet buffer stores packets for retransmission
- [x] Retransmit handler returns correct packets
- [x] Marker bit set on first packet after flush
- [x] Sequence numbers increment correctly
- [x] Timestamps increment by samples-per-packet
- [x] All unit tests pass
- [x] Integration tests pass
