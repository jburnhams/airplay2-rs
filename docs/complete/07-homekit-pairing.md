# Section 07: HomeKit Pairing Protocol

## Dependencies
- **Section 01**: Project Setup & CI/CD (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)
- **Section 03**: Binary Plist Codec (must be complete)
- **Section 04**: Cryptographic Primitives (must be complete)

## Overview

AirPlay 2 devices require HomeKit-style pairing for authentication. This section implements the pairing protocols:

1. **Transient Pairing**: Quick pairing without persistent keys (used for most connections)
2. **Pair-Setup**: Initial PIN-based pairing to establish long-term keys
3. **Pair-Verify**: Fast verification using previously established keys

The pairing protocol uses:
- SRP-6a for PIN verification
- Ed25519 for signatures
- X25519 for key exchange
- HKDF for key derivation
- ChaCha20-Poly1305 for encrypted messages

## Objectives

- Implement transient pairing (most common case)
- Implement Pair-Setup for PIN-based pairing
- Implement Pair-Verify for persistent pairing
- Handle encrypted session establishment
- Support pairing key storage (optional)

---

## Tasks

### 7.1 Pairing Types and State

- [x] **7.1.1** Define pairing state machine

**File:** `src/protocol/pairing/mod.rs`

...

### 7.2 TLV Encoding

- [x] **7.2.1** Implement TLV (Type-Length-Value) codec

**File:** `src/protocol/pairing/tlv.rs`

...

### 7.3 Transient Pairing

- [x] **7.3.1** Implement transient pairing (no PIN required)

**File:** `src/protocol/pairing/transient.rs`

...

### 7.4 Pair-Setup (PIN-based)

- [x] **7.4.1** Implement Pair-Setup with SRP

**File:** `src/protocol/pairing/setup.rs`

...

### 7.5 Pair-Verify

- [x] **7.5.1** Implement Pair-Verify for stored keys

**File:** `src/protocol/pairing/verify.rs`

...

### 7.6 Pairing Storage

- [x] **7.6.1** Implement pairing key storage

**File:** `src/protocol/pairing/storage.rs`

...

## Acceptance Criteria

- [x] TLV encoding/decoding handles fragmentation correctly
- [x] Transient pairing produces valid M1/M3 messages
- [x] Pair-Setup integrates with SRP correctly
- [x] Pair-Verify validates device signatures
- [x] Session keys are derived correctly
- [x] Encrypted channel encrypts/decrypts messages
- [x] Storage interface is implemented
- [x] All unit tests pass
- [x] Error handling covers all failure modes

---

## Notes

- The exact HKDF salt/info strings may need adjustment based on protocol analysis
- SRP integration requires careful handling of big integers
- Device signature verification is critical for security
- Consider adding rate limiting for failed pairing attempts
