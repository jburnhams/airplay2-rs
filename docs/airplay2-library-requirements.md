# AirPlay 2 Rust Library Requirements

## Overview

This document specifies the requirements for a standalone Rust library that enables streaming audio to AirPlay 2 compatible devices (HomePod, HomePod Mini, Apple TV, AirPort Express, and third-party AirPlay 2 speakers).

The library should be designed as a general-purpose crate that can be used by any Rust application, with this music player being the primary consumer.

## Scope

### In Scope
- **Audio streaming** to AirPlay 2 devices (sender/client functionality)
- **Device discovery** via mDNS/Bonjour
- **Playback control** (play, pause, stop, seek, next, previous)
- **Queue management** for track lists
- **Playback state** reporting
- **Volume control**
- **Multi-room audio** support (AirPlay 2's key feature)

### Out of Scope
- AirPlay receiver functionality (acting as a speaker)
- Video/screen mirroring
- AirPlay 1 (legacy RAOP) support (though could be added later)
- Photo streaming

---

## Functional Requirements

### 1. Device Discovery

The library must provide device discovery functionality.

```rust
/// Discover AirPlay 2 devices on the local network
pub async fn discover() -> impl Stream<Item = AirPlayDevice>;

/// One-shot scan that returns after timeout
pub async fn scan(timeout: Duration) -> Vec<AirPlayDevice>;
```

**Device Information:**
```rust
pub struct AirPlayDevice {
    /// Unique device identifier
    pub id: String,

    /// Human-readable device name (e.g., "Living Room HomePod")
    pub name: String,

    /// Device model (e.g., "HomePod Mini", "Apple TV 4K")
    pub model: Option<String>,

    /// IP address
    pub address: IpAddr,

    /// Service port
    pub port: u16,

    /// Whether device supports AirPlay 2 features
    pub supports_airplay2: bool,

    /// Whether device supports multi-room/grouped playback
    pub supports_grouping: bool,

    /// Current volume level (0.0 - 1.0) if available
    pub volume: Option<f32>,
}
```

**Requirements:**
- Must discover devices advertising `_airplay._tcp.local.` service
- Must parse TXT records for device capabilities
- Must support continuous discovery (stream) and one-shot scanning
- Must handle devices appearing/disappearing from network
- Should deduplicate devices by ID

---

### 2. Connection Management

```rust
/// Connect to an AirPlay 2 device
pub async fn connect(device: &AirPlayDevice) -> Result<AirPlayClient, AirPlayError>;

/// AirPlay client for controlling playback
pub struct AirPlayClient {
    // ...
}

impl AirPlayClient {
    /// Check if connection is still active
    pub fn is_connected(&self) -> bool;

    /// Gracefully disconnect from device
    pub async fn disconnect(&mut self) -> Result<(), AirPlayError>;

    /// Get the connected device info
    pub fn device(&self) -> &AirPlayDevice;
}
```

**Requirements:**
- Must handle AirPlay 2 authentication/pairing (HomeKit transient pairing)
- Must maintain persistent connection during playback
- Must handle connection drops gracefully with reconnection capability
- Each `AirPlayClient` instance manages a single device connection; the library must support creating multiple independent `AirPlayClient` instances for multi-room scenarios (orchestrated via `AirPlayGroup`)
- Should provide connection state change callbacks/events

---

### 3. Audio Streaming

The library must support **URL-based streaming** where the AirPlay device fetches audio from a provided URL.

```rust
impl AirPlayClient {
    /// Load and play a single track from URL
    pub async fn load(&mut self, track: &TrackInfo) -> Result<(), AirPlayError>;

    /// Load multiple tracks as a queue
    pub async fn load_queue(
        &mut self,
        tracks: &[TrackInfo],
        start_index: usize
    ) -> Result<(), AirPlayError>;

    /// Add track to play next (after current track)
    pub async fn add_next(&mut self, track: &TrackInfo) -> Result<(), AirPlayError>;

    /// Append track to end of queue
    pub async fn add_to_queue(&mut self, track: &TrackInfo) -> Result<(), AirPlayError>;
}

/// Track information for streaming
pub struct TrackInfo {
    /// URL to audio file (must be accessible by the AirPlay device)
    pub url: String,

    /// Track title
    pub title: String,

    /// Artist name
    pub artist: String,

    /// Album name
    pub album: Option<String>,

    /// Album artwork URL
    pub artwork_url: Option<String>,

    /// Track duration in seconds
    pub duration: Option<f32>,

    /// Track number on album
    pub track_number: Option<u32>,

    /// Disc number
    pub disc_number: Option<u32>,
}
```

**Requirements:**
- Must support HTTP/HTTPS URLs for audio content
- Must support common audio formats: MP3, AAC, ALAC, FLAC, WAV
- Must transmit track metadata (title, artist, album, artwork)
- Must handle queue management on the device
- Should support gapless playback between tracks

---

### 4. Playback Control

```rust
impl AirPlayClient {
    /// Start/resume playback
    pub async fn play(&mut self) -> Result<(), AirPlayError>;

    /// Pause playback
    pub async fn pause(&mut self) -> Result<(), AirPlayError>;

    /// Stop playback and clear queue
    pub async fn stop(&mut self) -> Result<(), AirPlayError>;

    /// Skip to next track
    pub async fn next(&mut self) -> Result<(), AirPlayError>;

    /// Go to previous track
    pub async fn previous(&mut self) -> Result<(), AirPlayError>;

    /// Seek to position in current track
    pub async fn seek(&mut self, position_secs: f32) -> Result<(), AirPlayError>;

    /// Play track at specific queue position
    pub async fn play_at(&mut self, index: usize) -> Result<(), AirPlayError>;

    /// Remove track from queue
    pub async fn remove_at(&mut self, index: usize) -> Result<(), AirPlayError>;

    /// Set volume (0.0 - 1.0)
    pub async fn set_volume(&mut self, volume: f32) -> Result<(), AirPlayError>;

    /// Get current volume
    pub async fn get_volume(&self) -> Result<f32, AirPlayError>;
}
```

**Requirements:**
- All playback commands must be responsive (< 500ms typical latency)
- Must handle rapid command sequences without race conditions
- Seek must be sample-accurate where supported by device

---

### 5. Playback State

```rust
impl AirPlayClient {
    /// Get current playback state
    pub async fn get_playback_state(&self) -> Result<PlaybackState, AirPlayError>;

    /// Subscribe to playback state changes
    pub fn subscribe_state(&self) -> impl Stream<Item = PlaybackState>;
}

pub struct PlaybackState {
    /// Whether audio is currently playing
    pub is_playing: bool,

    /// Current track info (None if queue empty)
    pub current_track: Option<TrackInfo>,

    /// Position in current track (seconds)
    pub position_secs: f32,

    /// Duration of current track (seconds)
    pub duration_secs: Option<f32>,

    /// Current volume (0.0 - 1.0)
    pub volume: f32,

    /// Current queue
    pub queue: Vec<TrackInfo>,

    /// Index of current track in queue (None if queue is empty)
    pub queue_index: Option<usize>,

    /// Shuffle enabled
    pub shuffle: bool,

    /// Repeat mode
    pub repeat: RepeatMode,
}

pub enum RepeatMode {
    Off,
    All,
    One,
}
```

**Requirements:**
- Must poll device state at configurable intervals (default ~500ms)
- Must provide event-based state updates via Stream
- Must accurately report playback position for UI progress bars
- Must detect track changes and queue modifications

---

### 6. Multi-Room Audio (AirPlay 2 Feature)

```rust
/// Create a group of devices for synchronized playback
pub async fn create_group(devices: &[AirPlayDevice]) -> Result<AirPlayGroup, AirPlayError>;

pub struct AirPlayGroup {
    // ...
}

impl AirPlayGroup {
    /// Get all devices in group
    pub fn devices(&self) -> &[AirPlayDevice];

    /// Add device to group
    pub async fn add_device(&mut self, device: &AirPlayDevice) -> Result<(), AirPlayError>;

    /// Remove device from group
    pub async fn remove_device(&mut self, device_id: &str) -> Result<(), AirPlayError>;

    /// Set volume for specific device in group
    pub async fn set_device_volume(
        &mut self,
        device_id: &str,
        volume: f32
    ) -> Result<(), AirPlayError>;

    /// All playback control methods (play, pause, load, etc.)
    // Same interface as AirPlayClient
}
```

**Requirements:**
- Must support synchronized playback across multiple devices
- Must handle per-device volume control within groups
- Must handle devices joining/leaving groups gracefully
- Audio must be synchronized within ~10ms across devices

---

## Non-Functional Requirements

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum AirPlayError {
    #[error("Device not found")]
    DeviceNotFound,

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Device disconnected")]
    Disconnected,

    #[error("Playback error: {0}")]
    PlaybackError(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] std::io::Error),

    #[error("Timeout")]
    Timeout,
}
```

### Async Runtime

- Must be runtime-agnostic (work with tokio, async-std, etc.)
- Should use `async-trait` for async trait methods
- All blocking operations must be async

### Thread Safety

- All public types must be `Send + Sync` where appropriate
- Must support concurrent access from multiple tasks
- Internal state must be properly synchronized

### Logging

- Must use `tracing` or `log` crate for logging
- Must provide debug-level logs for protocol messages
- Must not log sensitive information (pairing keys, etc.)

### Configuration

```rust
pub struct AirPlayConfig {
    /// Timeout for device discovery (default: 5 seconds)
    pub discovery_timeout: Duration,

    /// Timeout for connection attempts (default: 10 seconds)
    pub connection_timeout: Duration,

    /// Interval for polling playback state (default: 500ms)
    pub state_poll_interval: Duration,

    /// Enable debug logging of protocol messages
    pub debug_protocol: bool,
}

impl Default for AirPlayConfig { /* sensible defaults */ }
```

---

## Integration API

For easy integration with applications like this music player, provide a high-level wrapper:

```rust
/// High-level client that matches the music-player's Player trait pattern
pub struct AirPlayPlayer {
    client: AirPlayClient,
    // ...
}

impl AirPlayPlayer {
    pub async fn connect(device: AirPlayDevice) -> Result<Self, AirPlayError>;

    // Methods matching the Player trait from music-player-addons:
    pub async fn play(&mut self) -> Result<(), AirPlayError>;
    pub async fn pause(&mut self) -> Result<(), AirPlayError>;
    pub async fn stop(&mut self) -> Result<(), AirPlayError>;
    pub async fn next(&mut self) -> Result<(), AirPlayError>;
    pub async fn previous(&mut self) -> Result<(), AirPlayError>;
    pub async fn seek(&mut self, position_ms: u32) -> Result<(), AirPlayError>;
    pub async fn load_tracks(
        &mut self,
        tracks: Vec<TrackInfo>,
        start_index: Option<usize>,
    ) -> Result<(), AirPlayError>;
    pub async fn play_next(&mut self, track: TrackInfo) -> Result<(), AirPlayError>;
    pub async fn get_current_playback(&self) -> Result<PlaybackInfo, AirPlayError>;
    pub async fn disconnect(&mut self) -> Result<(), AirPlayError>;
    pub fn device_type(&self) -> &'static str; // Returns "AirPlay"
}

/// Playback info matching music-player's Playback struct
pub struct PlaybackInfo {
    pub current_track: Option<TrackInfo>,
    pub index: u32,
    pub position_ms: u32,
    pub is_playing: bool,
    /// Queue items as (track, item_id) tuples where item_id is a unique
    /// identifier for the track's position in the queue (used for queue
    /// manipulation operations like remove, reorder, play-at)
    pub items: Vec<(TrackInfo, i32)>,
}
```

> **Note on `disconnect`:** The existing `Player` trait in this music player has `disconnect`
> as synchronous. For the standalone library, `disconnect` should be async since it involves
> network I/O. When integrating with the music player, the addon implementation can handle
> this by either:
> 1. Spawning the async disconnect in a background task (fire-and-forget)
> 2. Using a command channel pattern (as Chromecast addon does)
> 3. Proposing an update to the `Player` trait to make `disconnect` async

---

## Crate Structure

```
airplay2/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API exports
│   ├── discovery.rs     # mDNS device discovery
│   ├── client.rs        # AirPlayClient implementation
│   ├── device.rs        # AirPlayDevice struct
│   ├── protocol/        # AirPlay 2 protocol implementation
│   │   ├── mod.rs
│   │   ├── auth.rs      # HomeKit pairing/authentication
│   │   ├── rtsp.rs      # RTSP session management
│   │   ├── rtp.rs       # RTP audio streaming
│   │   └── fairplay.rs  # FairPlay encryption (if needed)
│   ├── player.rs        # High-level AirPlayPlayer wrapper
│   ├── group.rs         # Multi-room group support
│   ├── error.rs         # Error types
│   └── config.rs        # Configuration
├── examples/
│   ├── discover.rs      # Device discovery example
│   ├── play_url.rs      # Simple URL playback
│   └── multi_room.rs    # Multi-room example
└── tests/
    └── ...
```

---

## Dependencies (Suggested)

```toml
[dependencies]
tokio = { version = "1", features = ["net", "sync", "time"] }
async-trait = "0.1"
futures = "0.3"
mdns-sd = "0.5"           # mDNS discovery
thiserror = "1.0"         # Error handling
tracing = "0.1"           # Logging
bytes = "1"               # Buffer handling
# Protocol-specific deps TBD based on implementation approach
```

---

## Testing Requirements

1. **Unit tests** for all public API methods
2. **Integration tests** with mock AirPlay device
3. **Example programs** demonstrating common use cases
4. **Documentation** with rustdoc examples for all public items

---

## Success Criteria

The library is considered complete when:

1. Can discover AirPlay 2 devices on local network
2. Can connect to and authenticate with HomePod/HomePod Mini
3. Can stream audio from HTTP URLs to device
4. Can control playback (play, pause, next, previous, seek)
5. Can report accurate playback state
6. Can manage a queue of tracks
7. Works with this music player's addon system
8. Has comprehensive documentation and examples

---

## References

- [AirPlay 2 Internals](https://emanuelecozzi.net/docs/airplay2) - Protocol documentation
- [openairplay/airplay2-receiver](https://github.com/openairplay/airplay2-receiver) - Python receiver implementation (protocol reference)
- [mikebrady/shairport-sync](https://github.com/mikebrady/shairport-sync) - AirPlay receiver in C
- [pyatv](https://pyatv.dev/) - Python AirPlay client library
