# AirPlay 2 HomePod Gen 1 Compatibility Findings

## Overview

This document summarizes the investigation and fixes required to enable MP3 playback from the `airplay2-rs` library to HomePod Gen 1 devices (model `AudioAccessory1,1`). The primary issue was that HomePod Gen 1 devices were timing out during the `SETUP` request, preventing any audio streaming.

---

## Key Discovery: PTP vs NTP Timing Protocol

### The Problem

HomePod Gen 1 devices were **timing out after 10 seconds** on the `SETUP` Step 1 request, never responding to connection attempts. The library was using **NTP (Network Time Protocol)** for timing synchronization, which works fine for Airport Express and other AirPlay devices but is rejected by HomePod Gen 1.

### The Solution

HomePod Gen 1 requires **PTP (Precision Time Protocol)** instead of NTP. When the `SETUP` Step 1 plist was changed from:

```diff
- .insert("timingProtocol", "NTP")
+ .insert("timingProtocol", "PTP")
+ .insert("timingPeerInfo", timing_peer_info)
```

The HomePod immediately responded with a successful `200 OK` and proceeded with the connection.

### Technical Details

**PTP Requirements:**
- Must include `timingPeerInfo` dictionary in `SETUP` Step 1 plist
- `timingPeerInfo` contains:
  - `Addresses`: Array with sender's IP address(es)
  - `ID`: Client session ID (UUID string)

**HomePod Response:**
- Returns rich `timingPeerInfo` structure including:
  - `ClockID`: Device PTP clock identifier
  - `ClockPorts`: Port mapping for PTP synchronization
  - `Addresses`: HomePod's network addresses (IPv4 and IPv6)
  - `DeviceType`: Integer identifier (2 for HomePod)

---

## Device Comparison

### Bedroom (AirPort Express / AirPort10,115)

| Characteristic | Value |
|---|---|
| **Model** | `AirPort10,115` |
| **Timing Protocol** | NTP ✅ |
| **SETUP Method** | Split (Session + Stream) |
| **Initial Volume** | Not reported in `/info` |
| **Connection Success** | ✅ Works with NTP |

### Kitchen (HomePod Gen 1 / AudioAccessory1,1)

| Characteristic | Value |
|---|---|
| **Model** | `AudioAccessory1,1` |
| **Firmware** | `925.5.1` (tvOS 18.3) |
| **Timing Protocol** | **PTP Only** ⚠️ |
| **SETUP Method** | Split (Session + Stream) |
| **Initial Volume** | `-15.375 dB` (quiet) |
| **Connection Success** | ✅ Works with PTP |
| **Volume Control** | ❌ 455 error before playback |

---

## Code Changes Made

### 1. Modified `src/connection/manager.rs`

**Location:** Lines 736-751 (SETUP Step 1 plist generation)

**Changes:**
```rust
// OLD: NTP timing
let setup_plist_step1 = DictBuilder::new()
    .insert("timingProtocol", "NTP")
    .insert("ekey", ek.to_vec())
    .insert("eiv", eiv.to_vec())
    .insert("et", 4)
    .build();

// NEW: PTP timing with peer info
let timing_peer_info = DictBuilder::new()
    .insert("Addresses", vec!["192.168.1.39".to_string()])
    .insert("ID", self.rtsp_session.lock().await.as_ref()
        .map(|s| s.client_session_id().to_string())
        .unwrap_or_default())
    .build();

let setup_plist_step1 = DictBuilder::new()
    .insert("timingProtocol", "PTP")
    .insert("timingPeerInfo", timing_peer_info)
    .insert("ekey", ek.to_vec())
    .insert("eiv", eiv.to_vec())
    .insert("et", 4)
    .build();
```

**Result:** HomePod Gen 1 now responds successfully to `SETUP` Step 1.

### 2. Session ID in URL Path

**Location:** `src/protocol/rtsp/session.rs` lines 131-142

**Change:** Modified `setup_session_request` to include session ID in path:
```rust
let path = format!("/{}", self.client_session_id);
```

This aligns with the AirPlay 2 specification format: `rtsp://<host>/<session-id>`

### 2. Session ID in URL Path

**Location:** `src/protocol/rtsp/session.rs` lines 131-142

**Change:** Modified `setup_session_request` to include session ID in path:
```rust
let path = format!("/{}", self.client_session_id);
```

This aligns with the AirPlay 2 specification format: `rtsp://<host>/<session-id>`

### 3. Removed Transport Header from SETUP Step 1

The `Transport` header was removed from `SETUP` Step 1 as it's not present in the HomePod specification examples. Transport negotiation happens in `SETUP` Step 2 instead.

### 4. Added File Playback Support

**New File:** `src/streaming/file.rs` (199 lines)

**Purpose:** Decode audio files (MP3, FLAC, etc.) using the Symphonia media framework.

**Key Components:**
```rust
pub struct FileSource {
    decoder: Box<dyn Decoder>,
    format: Box<dyn FormatReader>,
    track_id: u32,
    buffer: Vec<i16>,
    buffer_pos: usize,
    audio_format: AudioFormat,
}

impl AudioSource for FileSource {
    fn format(&self) -> AudioFormat { /* ... */ }
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> { /* ... */ }
}
```

**Features:**
- Auto-detects file format (MP3, FLAC, WAV, etc.)
- Decodes to PCM i16 samples
- Handles multiple sample formats (U8, S16, F32)
- Interleaves multi-channel audio for streaming
- Reports format to streaming pipeline (sample rate, channels)

### 5. Exposed Streaming Modules

**Location:** `src/streaming/mod.rs`

**Changes:**
```diff
-mod source;
+pub mod source;
+#[cfg(feature = "decoders")]
+pub mod file;
```

Made `source` module public and added conditional `file` module for decoder support.

### 6. Added `play_file()` Method to AirPlayPlayer

**Location:** `src/player/mod.rs`

**New Method:**
```rust
#[cfg(feature = "decoders")]
pub async fn play_file(&mut self, path: impl AsRef<Path>) -> Result<(), AirPlayError> {
    use crate::streaming::file::FileSource;
    let source = FileSource::new(path).map_err(|e| AirPlayError::IoError {
        message: e.to_string(),
    })?;
    
    self.client.stream_audio(source).await
}
```

**Purpose:** Convenience method to play local audio files without manual decoder setup.

**Usage:**
```rust
let mut player = AirPlayPlayer::new();
player.connect_by_name("Kitchen", Duration::from_secs(3)).await?;
player.play_file("song.mp3").await?;
```

---

## Volume Control Discovery

### Issue: 455 "Method Not Valid In This State"

When attempting to set volume **before playback starts**, the HomePod returns:

```
RTSP/1.0 455 Method Not Valid In This State
```

This applies to **both** volume and `RECORD` commands.

### Current Behavior

1. **Initial Volume:** HomePod reports `initialVolume: -15.375 dB` in `GET /info` response
2. **SET_PARAMETER (volume):** Returns 455 when called after `SETUP` but before streaming
3. **RECORD:** Returns 455 (playback likely auto-starts on `SETUP` completion)

### Workaround

Audio plays at the default `-15dB` volume. Users must manually adjust HomePod volume via:
- Siri voice commands
- Home app
- Physical touch controls

---

## Audio Streaming Verification (Feb 2026 Update)

### Current Status (Session 2)

**Connection:** ✅ Working — connects to Kitchen HomePod in ~2 seconds
**SETUP Step 1:** ✅ HomePod responds 200 OK (empty dict `{}` — no eventPort/timingPort for PTP mode)
**SETUP Step 2:** ✅ HomePod returns audio/control ports in `streams` array
**RECORD:** ✅ Accepted by device (200 OK)
**RTP Streaming:** ✅ Packets flowing — 1800+ packets sent per song
**Volume Control:** ❌ Still 455 "Method Not Valid In This State"
**Audio Output:** ❌ No sound heard on HomePod

### Bug Fixes Applied This Session

1. **Device Name Discovery** — AirPlay 2 `_airplay._tcp` devices were showing model names (e.g., `AudioAccessory1,1`) instead of friendly names (e.g., `Kitchen`). Fixed in `src/discovery/browser.rs` to extract the service instance name from the mDNS fullname.

2. **Premature RECORD in connect_internal** — RECORD was being sent during connection (blocking the 10s timeout). HomePod doesn't accept RECORD until streaming starts. Moved RECORD to `stream_audio()` where it's sent after a 100ms delay as a background task.

3. **PTP Handler Not Communicating** — The PTP handler was binding to ephemeral port `0.0.0.0:0` and "connecting" to `device_ip:0` (timing port was 0 for PTP mode). **No PTP Sync/Delay_Req/Delay_Resp messages were exchanged.** Fixed to use standard IEEE 1588 ports 319 (event) and 320 (general).

### Root Cause Analysis: Why No Audio Despite RTP Flowing

The HomePod receives RTP packets but does **not** play them because PTP clock synchronization is not working. The device needs valid PTP timing to:
- Synchronize its DAC with the RTP timestamp stream
- Know when to schedule audio playback
- Validate that the sender is a legitimate PTP master

**Evidence:**
- SETUP Step 1 returns empty dict — no `timingPort` (expected for PTP mode per docs)
- PTP handler logged NO incoming Delay_Req messages from device
- PTP handler logged NO outgoing Sync messages (connected to port 0)
- Volume SET_PARAMETER returns 455 — device never enters "playing" state

### PTP Port Requirements

AirPlay 2 PTP uses **standard IEEE 1588 ports**:
- **Port 319** — Event messages (Sync, Delay_Req)
- **Port 320** — General messages (Follow_Up, Delay_Resp)

These are **privileged ports** requiring elevated/administrator access. The fix binds to these ports and sends Sync messages to the device on port 319.

**Reference:** [Shairport Sync NQPTP](https://github.com/mikebrady/shairport-sync) — uses companion daemon `NQPTP` on ports 319/320

### SETUP Response Analysis

**Step 1 Response (PTP mode):**
```
bplist00 — 42 bytes
Dictionary({})  // Empty — no eventPort, no timingPort (expected for PTP)
```

**Step 2 Response:**
```
streams: [
  {
    arrivalToRenderLatencyMs: 86,
    dataPort: -1750  (= 63786 as u16),
    controlPort: -13569  (= 51967 as u16),
    type: 96  (buffered audio)
  }
]
```
Note: Port values are signed integers in plist, cast to u16 correctly via two's complement.

### Next Steps (Session 2)

- [x] Test with elevated privileges to bind ports 319/320
- [x] Verify PTP Sync messages reach the HomePod
- [ ] ~~Confirm device sends Delay_Req responses~~ (not needed — see Session 3)
- [ ] Check if audio plays once PTP is working
- [x] Consider whether HomePod expects us to be PTP **slave** not master → **master** is correct

---

## Session 3: PTP Clock Synchronization Deep Dive (Feb 2026)

### Overview

This session focused on getting PTP clock synchronization working properly between the AirPlay sender (us) and the HomePod Gen 1 receiver. While connections succeed and RTP packets flow, the HomePod produces no audio output because PTP timing is not synchronized.

### Key Discovery: PTP Role — Client Is Master

**Problem:** Initial confusion about whether we should be PTP master or slave.

**Answer:** The AirPlay **sender/client is the PTP master**. The HomePod is the PTP **slave**. Evidence:

1. **NQPTP** (shairport-sync's PTP daemon) is a passive listener — it monitors PTP Sync/Follow_Up from the master clock on ports 319/320 but never sends Delay_Req. It exists on the **receiver** side.
2. **BMCA (Best Master Clock Algorithm):** Both sides send Announce messages. The side with lower `priority1` wins master. We send `priority1=128`, HomePod sends `priority1=248`. Lower = better, so **we should win** the election.
3. **SETPEERS** tells the HomePod which IP addresses to listen for PTP on — it expects the sender to be providing the PTP clock.

### PTP Protocol Flow (What We Observe)

**Our outgoing PTP messages (port 319 event, 320 general):**
| Message | Port | Interval | Content |
|---------|------|----------|---------|
| Sync | 319 → HomePod:319 | 1 second | Two-step flag, T1 from `PtpTimestamp::now()` (Unix epoch) |
| Follow_Up | 320 → HomePod:320 | 1 second | Precise T1 timestamp |
| Announce | 320 → HomePod:320 | 2 seconds | GM=our clock_id, priority1=128 |

**HomePod incoming PTP messages:**
| Message | Port | Interval | Content |
|---------|------|----------|---------|
| Sync | 319 | ~125ms (8 Hz) | Two-step flag, T1=0 (placeholder) |
| Follow_Up | 320 | ~125ms (8 Hz) | T1=488882.xxx (device uptime, NOT Unix epoch) |
| Announce | 320 | ~250ms (4 Hz) | GM=0x50BC96E699860008, priority1=248 |
| Signaling | 320 | ~250ms | IEEE 1588 Signaling (type 0x0C) |

**Missing (not observed from either side):**
- Delay_Req (neither side sends)
- Delay_Resp (neither side sends)

### Current State of PTP

**Both sides are acting as PTP master simultaneously.** The HomePod has not accepted us as master despite our better priority (128 < 248). It continues sending its own Sync/Follow_Up/Announce at 8Hz.

**Possible reasons the BMCA election isn't resolving:**
1. Our Announce body format may be slightly wrong (IEEE 1588 Announce body is 30 bytes with specific fields — we may be missing or mis-ordering fields)
2. The HomePod may require a specific `domain_number` in the PTP header (we send 0)
3. The HomePod may ignore BMCA and always act as both master and slave simultaneously
4. There may be a minimum number of Announce messages required before the HomePod transitions

### SETUP Step 1 Response (With PTP — Rich Response)

After removing the redundant first SETUP (which caused empty `{}` responses), SETUP Step 1 now returns:

```
{
    "eventPort": 49849,       // (stored as -15687 signed)
    "timingPeerInfo": {
        "Addresses": ["192.168.0.100", "fe80::1874:a924:a5ba:8bed"],
        "SupportsClockPortMatchingOverride": true,
        "DeviceType": 2,
        "ClockID": 5817690735818178568,  // 0x50B48E056F3B2988
        "ClockPorts": {
            "8E056F3B29808116": -32728   // port 32808 as u16
        },
        "ID": "160D7072-C9EF-49F6-9A76-A69514ED7003"
    }
}
```

**Notable:** `ClockPorts` maps clock IDs to non-standard ports (32808 instead of 319/320). The `SupportsClockPortMatchingOverride: true` flag suggests the device can accept PTP on ports other than those in ClockPorts. We currently use standard ports 319/320 and the HomePod does send/receive on them.

### Protocol Flow Changes (Session 3)

**Removed:** ANNOUNCE (AirPlay 1 SDP-based — HomePod returns 455)
**Removed:** Redundant first SETUP (groupUUID/macAddress — merged into SETUP Step 1 for PTP)
**Added:** SETPEERS (binary plist with peer IP addresses — returns 200)
**Added:** RECORD in connection flow (after SETPEERS) with 5-second timeout

**Current AirPlay 2 sequence:**
```
OPTIONS → GET /info → pair-setup → pair-verify →
SETUP Step 1 (PTP + encryption) → SETUP Step 2 (stream format) →
SETPEERS → PTP start → RECORD (5s timeout) → stream_audio() →
RECORD (background, after 100ms delay with audio flowing) → FLUSH (TODO) → Audio data
```

**RECORD behavior:**
- RECORD sent in `connect_internal()` before streaming: **times out** (HomePod doesn't respond until audio flows)
- RECORD sent from `stream_audio()` after 100ms with RTP packets already flowing: **returns 200 OK**
- This confirms: HomePod requires audio data on the data port before it will accept RECORD

### Audio Status

| Component | Status | Detail |
|-----------|--------|--------|
| SETUP Step 1 | ✅ 200 | Rich response with timingPeerInfo, eventPort, ClockPorts |
| SETUP Step 2 | ✅ 200 | streams[0]: dataPort=57240, controlPort=58168, type=96 (buffered) |
| SETPEERS | ✅ 200 | Peer list with client IP + device IP |
| PTP Sync/Follow_Up | ⚠️ Partial | Both sides sending independently — no sync achieved |
| PTP Announce | ⚠️ Competing | Both sides claim master — BMCA not resolving |
| RECORD | ✅ 200 | Accepted after audio starts flowing |
| RTP Packets | ✅ Flowing | 10000+ packets sent per song |
| FLUSH | ❌ Missing | Not yet implemented in connection flow |
| Audio Output | ❌ Silent | HomePod receives packets but plays nothing |

### Root Cause Analysis: Why No Audio

The most likely cause is **PTP clock synchronization failure**. The HomePod needs a synchronized PTP clock to:
1. Convert RTP timestamps to wall-clock time for audio scheduling
2. Know when to render each audio sample
3. Validate that the sender has a coherent timing reference

Without PTP sync, the HomePod has no way to schedule playback of the buffered audio packets, so it silently drops them.

**Secondary issue:** FLUSH is not being sent. The protocol requires `FLUSH` with `RTP-Info: seq=<first_seq>;rtptime=<first_timestamp>` immediately before audio data to tell the HomePod which RTP sequence number and timestamp to expect first.

### PTP Architecture

```
┌─────────────────────┐         ┌─────────────────────┐
│  AirPlay Sender     │         │  HomePod Gen 1      │
│  (airplay2-rs)      │         │  (AudioAccessory1,1)│
│                     │         │                     │
│  PTP Master         │         │  PTP Slave           │
│  priority1 = 128    │         │  priority1 = 248    │
│  (should win BMCA)  │         │  (should lose BMCA) │
│                     │         │                     │
│  Sends:             │ ──319─▶ │  Receives:          │
│   Sync (1Hz)        │         │   Sync              │
│   Follow_Up (320)   │ ──320─▶ │   Follow_Up         │
│   Announce (320)    │ ──320─▶ │   Announce          │
│                     │         │                     │
│  Receives:          │ ◀─319── │  Also sends:        │
│   Sync (8Hz)        │         │   Sync (8Hz)        │
│   Follow_Up (320)   │ ◀─320── │   Follow_Up         │
│   Announce (320)    │ ◀─320── │   Announce           │
│   Signaling (320)   │ ◀─320── │   Signaling         │
│                     │         │                     │
│  NOT observed:      │         │  NOT observed:      │
│   Delay_Req ────────│─ ✗ ────│─► Delay_Resp        │
│   Delay_Resp ◀──────│─ ✗ ────│── Delay_Req         │
└─────────────────────┘         └─────────────────────┘
```

**Both sides currently act as master. BMCA should resolve this but hasn't yet.**

### Next Steps

- [ ] Debug why BMCA isn't resolving — verify our Announce format matches IEEE 1588 spec exactly
- [ ] Try matching HomePod's Announce rate (4Hz instead of 0.5Hz)
- [ ] Consider whether HomePod requires a specific `domain_number` in PTP header
- [ ] Add FLUSH command after RECORD and before audio data
- [ ] Investigate whether HomePod's `ClockPorts` port (32808) should be used instead of 319/320
- [ ] Check if HomePod needs to see consistent clock_id between SETUP timingPeerInfo and PTP messages

---

## Future Work & Recommendations

### 1. Device-Specific Protocol Selection

**Problem:** Current implementation hardcodes PTP for all devices.

**Solution:** Implement device detection and protocol selection:

```rust
fn select_timing_protocol(device_model: &str) -> TimingProtocol {
    match device_model {
        "AudioAccessory1,1" | "AudioAccessory1,2" => TimingProtocol::PTP,
        "AudioAccessory5,1" => TimingProtocol::PTP, // HomePod mini
        _ => TimingProtocol::NTP, // AirPort, AppleTV, etc.
    }
}
```

### 2. Dynamic Volume Control

**Current Limitation:** Cannot set volume before playback starts.

**Potential Solutions:**

**Option A:** Delayed volume setting
```rust
// Start playback task
let playback_handle = tokio::spawn(player.play_file(path));

// Set volume after brief delay (once playback active)
tokio::time::sleep(Duration::from_millis(500)).await;
player.set_volume(1.0).await?;
```

**Option B:** Volume control API during playback
```rust
// New API method
player.play_file_with_options(path, PlaybackOptions {
    initial_volume: Some(1.0),
    start_paused: false,
}).await?;
```

### 3. Protocol State Machine

Implement proper RTSP state tracking to prevent 455 errors:

```rust
enum RtspState {
    Connected,
    Announced,
    SetupComplete,
    Playing,
    Paused,
}

impl ConnectionManager {
    fn can_set_volume(&self) -> bool {
        matches!(self.state, RtspState::Playing | RtspState::Paused)
    }
}
```

### 4. GET /info Volume Verification

Add verification step to confirm volume changes:

```rust
async fn verify_volume_change(&mut self, target: f32) -> Result<f32> {
    self.set_volume(target).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let info = self.get_info().await?;
    if let Some(vol) = info.get("initialVolume") {
        return Ok(vol.as_f64() as f32);
    }
    Err(Error::VolumeVerificationFailed)
}
```

### 5. Comprehensive Device Testing

Test matrix for timing protocol compatibility:

| Device | Model | Test NTP | Test PTP | Notes |
|---|---|---|---|---|
| HomePod Gen 1 | AudioAccessory1,1 | ❌ Timeout | ✅ Works | Requires PTP |
| HomePod mini | AudioAccessory5,1 | ❓ Unknown | ❓ Unknown | Likely PTP |
| HomePod 2 | AudioAccessory6,1 | ❓ Unknown | ❓ Unknown | Likely PTP |
| AirPort Express | AirPort10,115 | ✅ Works | ❓ Unknown | Known NTP device |
| Apple TV 4K | AppleTV11,1 | ❓ Unknown | ❓ Unknown | Test needed |

### 6. PTP Clock Synchronization

**Current:** Library sends `timingPeerInfo` but doesn't implement full PTP stack.

**Future:** Implement actual PTP clock synchronization for:
- Multi-room audio sync
- Lower latency playback
- Better timestamp accuracy

**Reference:** IEEE 1588 Precision Time Protocol

### 7. Auto-Discovery of Capabilities

Parse HomePod's `GET /info` response to auto-configure:

```rust
struct DeviceCapabilities {
    timing_protocols: Vec<TimingProtocol>, // Inferred from model
    supports_volume_control: bool,
    volume_control_timing: VolumeControlRequirement,
    initial_volume_db: Option<f32>,
}

impl DeviceCapabilities {
    fn from_info_response(info: &Plist) -> Self {
        let model = info.get_string("model");
        let features = info.get_integer("features");
        
        // Parse capabilities from device info
        // ...
    }
}
```

### 8. Error Recovery & Resilience

Add retry logic for common failure modes:

```rust
async fn setup_with_retry(&mut self) -> Result<()> {
    // Try PTP first (newer devices)
    match self.setup_with_timing(TimingProtocol::PTP).await {
        Ok(_) => return Ok(()),
        Err(SetupError::Timeout) => {
            // Fall back to NTP for older devices
            self.setup_with_timing(TimingProtocol::NTP).await
        }
        Err(e) => Err(e),
    }
}
```

### 9. Documentation Updates

Update library documentation with:
- Device compatibility matrix
- Timing protocol requirements
- Volume control limitations
- Best practices for HomePod support

### 10. Integration Tests

Add device-specific integration tests:

```rust
#[tokio::test]
#[ignore] // Requires physical HomePod
async fn test_homepod_gen1_ptp_connection() {
    let mut manager = ConnectionManager::new(/* ... */);
    let result = manager.connect_to_homepod_gen1().await;
    assert!(result.is_ok());
}
```

---

## References

- **AirPlay 2 Protocol Documentation:** `airplay2-homepod.md`
- **RTSP Specification:** RFC 2326
- **PTP Specification:** IEEE 1588-2008
- **Debug Logs:** `debug_output_102.txt` (PTP success), `debug_output_101.txt` (NTP timeout)

---

## Conclusion

**Connection:** Fully working — SETUP, SETPEERS, RECORD all return 200 OK.

**PTP Timing:** Partially working — both sides exchange Sync/Follow_Up/Announce, but BMCA election hasn't resolved. Both sides still act as master. No Delay_Req/Delay_Resp exchange occurs.

**Audio Playback:** Not working — RTP packets flow but HomePod produces no sound. Root cause is almost certainly PTP clock synchronization failure. The HomePod cannot schedule audio playback without a synchronized clock reference.

**Status:** ⚠️ Connection and RTP streaming work. PTP clock sync and audio output remain unresolved.
