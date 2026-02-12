# Section 49: HomeKit Pairing (Server-Side)

## Dependencies
- **Section 06**: HomeKit Pairing & Encryption (client-side primitives)
- **Section 46**: AirPlay 2 Receiver Overview
- **Section 48**: RTSP/HTTP Server Extensions
- **Section 04**: Cryptographic Primitives

## Overview

This section implements the **server-side** of HomeKit pairing for the AirPlay 2 receiver. The existing pairing module (Section 06) implements the client role; here we implement the server/responder role.

In HomeKit pairing:
- **Client** (iOS/macOS sender): Initiates pairing, sends M1/M3 messages
- **Server** (our receiver): Responds with M2/M4 messages, validates client

The pairing protocol uses SRP-6a (Secure Remote Password) for key exchange, then Ed25519 for identity verification.

### Protocol Flow

```
Sender (Client)                     Receiver (Server)
      │                                    │
      │─── POST /pair-setup ──────────────▶│
      │    M1: Method=0, User="Pair-Setup" │
      │                                    │
      │◀── Response ──────────────────────│
      │    M2: Salt, ServerPublic (B)     │
      │                                    │
      │─── POST /pair-setup ──────────────▶│
      │    M3: ClientPublic (A), Proof    │
      │                                    │
      │◀── Response ──────────────────────│
      │    M4: ServerProof, EncryptedData │
      │        (includes Ed25519 pubkey)  │
      │                                    │
      │════ Pair-Verify begins ════════════│
      │                                    │
      │─── POST /pair-verify ─────────────▶│
      │    M1: ClientPublicX25519, EncData│
      │                                    │
      │◀── Response ──────────────────────│
      │    M2: ServerPublicX25519, EncData│
      │                                    │
      │─── POST /pair-verify ─────────────▶│
      │    M3: EncryptedSignature         │
      │                                    │
      │◀── Response ──────────────────────│
      │    M4: EncryptedSignature         │
      │                                    │
      │════ Session keys established ══════│
```

## Objectives

- Implement SRP-6a server (verifier) role
- Generate and store password verifier from device PIN/password
- Handle /pair-setup endpoint (M1-M4)
- Handle /pair-verify endpoint (M1-M4)
- Derive session encryption keys after successful pairing
- Reuse existing cryptographic primitives (SRP, Ed25519, X25519, ChaCha20)
- Support both transient (PIN) and persistent pairing

---

## Tasks

### 49.1 SRP Server Implementation

- [x] **49.1.1** Implement SRP-6a server/verifier

### 49.2 TLV Types Extension

- [x] **49.2.1** Ensure TLV types cover all pairing needs

### 49.3 Endpoint Handlers

- [x] **49.3.1** Implement /pair-setup and /pair-verify handlers

### 49.4 Pairing Server Tests

- [x] **49.4.1** Test SRP server operations

### 49.5 Full Pairing Flow Tests

- [x] **49.5.1** Test complete pairing handshake

---

## Acceptance Criteria

- [x] SRP server correctly handles M1 and generates M2 with salt and public key
- [x] SRP server validates client proof in M3 and generates M4 with server proof
- [x] Wrong password results in authentication failure at M3
- [x] Pair-verify M1/M2 exchange completes successfully
- [x] Pair-verify M3/M4 exchange derives session keys
- [x] Encryption keys are available after successful pairing
- [x] State machine prevents out-of-order messages
- [x] Reset clears all pairing state
- [x] TLV encoding/decoding is compatible with iOS clients
- [x] All unit tests pass
- [x] Integration tests pass

---

## Notes

### Reuse from Client Implementation

The following are directly reused from the client pairing module (Section 06):
- `SrpParams::RFC5054_3072` - SRP parameters
- `TlvEncoder`/`TlvDecoder` - TLV message format
- `Ed25519Keypair` - Identity keys
- `X25519Keypair` - Session key exchange
- `ChaCha20Poly1305` - Encryption
- `HkdfSha512` - Key derivation

The server role differs primarily in:
1. Computing the SRP verifier (not the password proof)
2. Responding to messages rather than initiating
3. Validating the client rather than proving to the server

### PIN Display

For transient pairing, the receiver needs to display a 4-digit PIN to the user.
This is outside the scope of the protocol handler - the application layer should
handle PIN generation and display.

### Persistent Pairing

After successful pairing, the client's Ed25519 public key should be stored for
future pair-verify sessions. The storage mechanism is application-specific.

---

## References

- [HomeKit Accessory Protocol Specification](https://developer.apple.com/homekit/)
- [Section 06: HomeKit Pairing & Encryption](./complete/06-homekit-pairing-encryption.md)
- [SRP-6a Specification](http://srp.stanford.edu/design.html)
- [RFC 5054: SRP for TLS](https://tools.ietf.org/html/rfc5054)
