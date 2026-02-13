# Section 55: PTP Timing Synchronization

## Dependencies
- **Section 52**: Multi-Phase SETUP Handler (timing channel)
- **Section 40**: Timing Synchronization (AirPlay 1 patterns)

## Overview

AirPlay 2 uses Precision Time Protocol (PTP, IEEE 1588) for timing synchronization, enabling accurate multi-room playback. The receiver must synchronize its clock with the sender for sample-accurate audio output.

### PTP vs NTP-style Timing

| Aspect | NTP-style (AirPlay 1) | PTP (AirPlay 2) |
|--------|----------------------|-----------------|
| Precision | ~1-10ms | <1ms |
| Protocol | Custom UDP | IEEE 1588 |
| Multi-room | Limited | Full support |
| Clock model | Offset only | Offset + rate |

## Objectives

- Implement PTP message parsing and generation
- Calculate clock offset from timing exchanges
- Estimate clock drift rate
- Provide timestamp conversion for audio output
- Support timing channel on allocated UDP port

---

## Tasks

### 55.1 PTP Clock Implementation

- [x] **55.1.1** Implement PTP clock synchronization

**Implementation:** `src/protocol/ptp/` (shared module, usable by both client and receiver)

**Files:**
- `src/protocol/ptp/mod.rs` — Module root and re-exports
- `src/protocol/ptp/timestamp.rs` — PTP timestamp (IEEE 1588 80-bit + AirPlay 48.16 compact)
- `src/protocol/ptp/message.rs` — IEEE 1588 message types, header, parsing, encoding; AirPlay compact packet
- `src/protocol/ptp/clock.rs` — Clock synchronization: offset calculation, drift estimation, median filter, RTT-based outlier rejection
- `src/protocol/ptp/handler.rs` — Async UDP handlers for master (client) and slave (receiver) roles

**Tests:**
- `src/protocol/ptp/tests/timestamp.rs` — 30+ tests: conversions, roundtrips, precision, edge cases
- `src/protocol/ptp/tests/message.rs` — 30+ tests: all message types, header encoding, AirPlay packets
- `src/protocol/ptp/tests/clock.rs` — 25+ tests: offset, drift, median filter, RTT rejection, RTP conversion
- `src/protocol/ptp/tests/handler.rs` — 10+ tests: loopback exchanges, master/slave handlers
- `tests/ptp_integration.rs` — 16 integration tests: full IEEE 1588 and AirPlay exchanges

---

## Acceptance Criteria

- [x] PTP messages parsed correctly
- [x] Clock offset calculated from timing exchanges
- [x] Drift rate estimated over time
- [x] Timestamp conversion between local and remote
- [x] Median filter for robustness
- [x] All unit tests pass

---

## References

- [IEEE 1588 PTP](https://www.nist.gov/el/intelligent-systems-division-73500/ieee-1588)
- [AirPlay 2 Timing Analysis](https://emanuelecozzi.net/docs/airplay2/timing/)
