# Documentation Deficiencies

Review of docs/complete against source code. This document identifies discrepancies between documentation and implementation that should be corrected.

---

## 02-core-types.md

### AirPlayDevice struct

| Issue | Doc | Source |
|-------|-----|--------|
| Address field | `address: IpAddr` (single) | `addresses: Vec<IpAddr>` (multiple) |
| txt_records visibility | `pub(crate)` | `pub` |
| Missing fields | - | `raop_port: Option<u16>`, `raop_capabilities: Option<RaopCapabilities>` |
| Missing method | - | `address()` returns preferred IP from list |
| Missing method | - | `supports_raop()` |

**Source:** `src/types/device.rs:7-113`

### QueueItem struct

| Issue | Doc | Source |
|-------|-----|--------|
| ID field | `item_id: i32` | `id: QueueItemId` (u64 wrapper struct) |
| Missing field | - | `original_position: usize` |
| Missing type | - | `QueueItemId` struct with `new()` |

**Source:** `src/types/track.rs:74-115`

### types/mod.rs exports

Missing from doc:
- `QueueItemId`
- `RaopCapabilities`, `RaopCodec`, `RaopEncryption`, `RaopMetadataType`

**Source:** `src/types/mod.rs:18-20`

### AirPlayError enum

Missing variants from doc:
- `Raop(#[from] RaopError)` - wraps RAOP-specific errors
- `InvalidParameter { name, message }` - invalid parameter error
- `IoError { message, source }` - general I/O error

Missing type from doc:
- `RaopError` enum with variants: `AuthenticationFailed`, `UnsupportedEncryption`, `SdpParseError`, `KeyExchangeFailed`, `EncryptionError`, `TimingSyncFailed`, `RetransmitBufferOverflow`

**Source:** `src/error.rs:4-34, 38-41, 224-240`

---

## 06-rtp-protocol.md

### mod.rs structure

Missing public modules:
- `pub mod packet_buffer`
- `pub mod raop`
- `pub mod raop_timing`

Missing export:
- `RtpEncryptionMode`

**Source:** `src/protocol/rtp/mod.rs:11-13, 25`

### RtpCodec struct

Missing fields from doc:
- `chacha_key: Option<[u8; 32]>`
- `encryption_mode: RtpEncryptionMode`
- `nonce_counter: u64`

Missing methods from doc:
- `set_chacha_encryption(key: [u8; 32])`
- `encryption_mode() -> RtpEncryptionMode`
- `encode_arbitrary_payload(data, output)` - for ALAC encoding

Missing constants:
- `TAG_SIZE: usize = 16`
- `NONCE_SIZE: usize = 8`

**Source:** `src/protocol/rtp/codec.rs:25-33, 49-57, 64-67, 91-100, 141-229`

### RtpCodecError enum

Missing variants:
- `EncryptionFailed(String)`
- `DecryptionFailed(String)`

**Source:** `src/protocol/rtp/codec.rs:17-22`

### RtpEncryptionMode enum

Not documented at all:
```
enum RtpEncryptionMode { None, Aes128Ctr, ChaCha20Poly1305 }
```

**Source:** `src/protocol/rtp/codec.rs:25-33`

### AudioPacketBuilder

Missing method:
- `with_chacha_encryption(key: [u8; 32])`

**Source:** `src/protocol/rtp/codec.rs:347-350`

---

## 17-state-events.md

### Task status

All tasks marked `[ ]` incomplete but implementation exists and is functional.

**Source:** `src/state/mod.rs`, `src/state/container.rs`, `src/state/events.rs`

### ClientState.playback field

| Issue | Doc | Source |
|-------|-----|--------|
| Type | `PlaybackState` (appears to be enum with `Stopped` variant) | `PlaybackState` is a struct with multiple fields |

Doc shows:
```rust
playback: PlaybackState::Stopped
```

Source shows:
```rust
playback: PlaybackState::default()  // PlaybackState is a struct
```

**Source:** `src/state/container.rs:35`, `src/types/state.rs:4-35`

---

## 18-volume-control.md

### Task status

All tasks marked `[ ]` incomplete but implementation is complete.

### send_volume method

Doc shows stub with TODO comment:
```rust
// TODO: Send SET_PARAMETER with volume
Ok(())
```

Source has full implementation using `connection.send_command()`.

**Source:** `src/control/volume.rs:258-274`

### get_device_volume method

Doc shows stub returning default:
```rust
Ok(Volume::DEFAULT)
```

Source has full implementation parsing device response.

**Source:** `src/control/volume.rs:277-308`

---

## 20-mock-server.md

### Task status

All tasks marked `[ ]` incomplete but implementation exists.

### Request parsing

Doc references `RtspCodec` but actual implementation uses custom `try_parse_request()` method.

**Source:** `src/testing/mock_server.rs:237-290`

---

## 37-receiver-session-management.md

### Task status

All tasks marked `[ ]` incomplete but implementation exists for session.rs. Session manager (session_manager.rs) appears partially implemented or structured differently.

**Source:** `src/receiver/session.rs`

---

## Summary

### Priority fixes

1. **02-core-types.md** - Multiple structural differences in core types that users rely on for API usage
2. **06-rtp-protocol.md** - Missing ChaCha20-Poly1305 encryption (AirPlay 2 requirement)

### Housekeeping fixes

3. **17, 18, 20, 37** - Update task checkboxes from `[ ]` to `[x]` for implemented features

### Impact

- Core types doc may mislead users about field names and types
- RTP doc omits AirPlay 2 encryption entirely
- Task status checkboxes create false impression of incomplete work
