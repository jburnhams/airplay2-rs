# Section 30: DACP Remote Control Protocol

## Dependencies
- **Section 27**: RTSP Session for RAOP (must be complete)
- **Section 02**: Core Types, Errors & Configuration (must be complete)

## Overview

DACP (Digital Audio Control Protocol) enables AirPlay receivers to send playback commands back to the client (iTunes/Music app). When streaming to an AirPlay device, the client advertises a DACP service that the receiver can use for remote control.

The receiver sends HTTP GET requests to control playback:
- Play/Pause/Stop
- Next/Previous track
- Fast forward/Rewind
- Volume adjustment
- Shuffle toggle

## Protocol Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    DACP Service Advertisement                    │
│                                                                  │
│  Client (iTunes)                            Receiver (AirPlay)  │
│    │                                              │              │
│    │  1. Advertise _dacp._tcp service            │              │
│    │     (iTunes_Ctrl_{DACP-ID})                 │              │
│    │                                              │              │
│    │  2. Include DACP-ID in RTSP headers         │              │
│    │─────────────────────────────────────────────>│              │
│    │                                              │              │
│    │  3. Receiver browses for matching service   │              │
│    │<─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│              │
│    │                                              │              │
│    │  4. Receiver sends commands via HTTP        │              │
│    │<─────────────────────────────────────────────│              │
│    │     GET /ctrl-int/1/playpause               │              │
│    │     Active-Remote: {token}                  │              │
│    │                                              │              │
│    │  5. Client executes command                 │              │
│    │─────────────────────────────────────────────>│              │
│    │     204 No Content                          │              │
└─────────────────────────────────────────────────────────────────┘
```

## Objectives

- Implement DACP service advertisement
- Handle incoming HTTP command requests
- Execute playback control commands
- Provide callback interface for command handling

---

## Tasks

### 30.1 DACP Service Types

- [ ] **30.1.1** Define DACP constants and types

**File:** `src/protocol/dacp/mod.rs`

```rust
//! DACP (Digital Audio Control Protocol) for AirPlay remote control

mod service;
mod commands;
mod server;

pub use service::{DacpService, DacpServiceConfig};
pub use commands::{DacpCommand, CommandResult};
pub use server::{DacpServer, DacpHandler};

/// DACP service type for mDNS
pub const DACP_SERVICE_TYPE: &str = "_dacp._tcp.local.";

/// Default DACP port
pub const DACP_DEFAULT_PORT: u16 = 3689;

/// DACP TXT record keys
pub mod txt_keys {
    /// TXT record version
    pub const TXTVERS: &str = "txtvers";
    /// DACP version
    pub const VER: &str = "Ver";
    /// Database ID
    pub const DBID: &str = "DbId";
    /// OS information
    pub const OSSI: &str = "OSsi";
}
```

- [ ] **30.1.2** Define DACP commands

**File:** `src/protocol/dacp/commands.rs`

```rust
//! DACP command definitions

/// DACP playback commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DacpCommand {
    /// Start/resume playback
    Play,
    /// Pause playback
    Pause,
    /// Toggle play/pause
    PlayPause,
    /// Resume from pause
    PlayResume,
    /// Stop playback
    Stop,
    /// Skip to next track
    NextItem,
    /// Go to previous track
    PrevItem,
    /// Begin fast forward
    BeginFastForward,
    /// Begin rewind
    BeginRewind,
    /// End fast forward/rewind
    PlayResume2,
    /// Increase volume
    VolumeUp,
    /// Decrease volume
    VolumeDown,
    /// Toggle mute
    MuteToggle,
    /// Shuffle songs
    ShuffleSongs,
}

impl DacpCommand {
    /// Parse from URL path
    pub fn from_path(path: &str) -> Option<Self> {
        // Path format: /ctrl-int/1/{command}
        let command = path.strip_prefix("/ctrl-int/1/")?;

        match command {
            "play" => Some(Self::Play),
            "pause" => Some(Self::Pause),
            "playpause" => Some(Self::PlayPause),
            "playresume" => Some(Self::PlayResume),
            "stop" => Some(Self::Stop),
            "nextitem" => Some(Self::NextItem),
            "previtem" => Some(Self::PrevItem),
            "beginff" => Some(Self::BeginFastForward),
            "beginrew" => Some(Self::BeginRewind),
            "playresume" => Some(Self::PlayResume2),
            "volumeup" => Some(Self::VolumeUp),
            "volumedown" => Some(Self::VolumeDown),
            "mutetoggle" => Some(Self::MuteToggle),
            "shuffle_songs" => Some(Self::ShuffleSongs),
            _ => None,
        }
    }

    /// Get URL path for command
    pub fn path(&self) -> &'static str {
        match self {
            Self::Play => "/ctrl-int/1/play",
            Self::Pause => "/ctrl-int/1/pause",
            Self::PlayPause => "/ctrl-int/1/playpause",
            Self::PlayResume | Self::PlayResume2 => "/ctrl-int/1/playresume",
            Self::Stop => "/ctrl-int/1/stop",
            Self::NextItem => "/ctrl-int/1/nextitem",
            Self::PrevItem => "/ctrl-int/1/previtem",
            Self::BeginFastForward => "/ctrl-int/1/beginff",
            Self::BeginRewind => "/ctrl-int/1/beginrew",
            Self::VolumeUp => "/ctrl-int/1/volumeup",
            Self::VolumeDown => "/ctrl-int/1/volumedown",
            Self::MuteToggle => "/ctrl-int/1/mutetoggle",
            Self::ShuffleSongs => "/ctrl-int/1/shuffle_songs",
        }
    }

    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::PlayPause => "Play/Pause",
            Self::PlayResume | Self::PlayResume2 => "Resume",
            Self::Stop => "Stop",
            Self::NextItem => "Next Track",
            Self::PrevItem => "Previous Track",
            Self::BeginFastForward => "Fast Forward",
            Self::BeginRewind => "Rewind",
            Self::VolumeUp => "Volume Up",
            Self::VolumeDown => "Volume Down",
            Self::MuteToggle => "Toggle Mute",
            Self::ShuffleSongs => "Shuffle",
        }
    }
}

/// Result of command execution
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// Command executed successfully
    Success,
    /// Command not supported
    NotSupported,
    /// Command failed
    Failed(String),
}
```

---

### 30.2 DACP Service Advertisement

- [ ] **30.2.1** Implement DACP service registration

**File:** `src/protocol/dacp/service.rs`

```rust
//! DACP service advertisement

use super::{DACP_SERVICE_TYPE, DACP_DEFAULT_PORT, txt_keys};
use std::collections::HashMap;

/// DACP service configuration
#[derive(Debug, Clone)]
pub struct DacpServiceConfig {
    /// DACP ID (64-bit identifier)
    pub dacp_id: String,
    /// Active remote token
    pub active_remote: String,
    /// Service port
    pub port: u16,
    /// Database ID (typically same as DACP ID)
    pub db_id: String,
}

impl DacpServiceConfig {
    /// Create new configuration with random identifiers
    pub fn new() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let dacp_id = format!("{:016X}", rng.gen::<u64>());
        let active_remote = rng.gen::<u32>().to_string();

        Self {
            dacp_id: dacp_id.clone(),
            active_remote,
            port: DACP_DEFAULT_PORT,
            db_id: dacp_id,
        }
    }

    /// Get service instance name (iTunes_Ctrl_{DACP_ID})
    pub fn instance_name(&self) -> String {
        format!("iTunes_Ctrl_{}", self.dacp_id)
    }

    /// Get TXT records for service advertisement
    pub fn txt_records(&self) -> HashMap<String, String> {
        let mut records = HashMap::new();
        records.insert(txt_keys::TXTVERS.to_string(), "1".to_string());
        records.insert(txt_keys::VER.to_string(), "131073".to_string());
        records.insert(txt_keys::DBID.to_string(), self.db_id.clone());
        records.insert(txt_keys::OSSI.to_string(), "0x1F5".to_string());
        records
    }
}

impl Default for DacpServiceConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// DACP service for mDNS registration
pub struct DacpService {
    /// Configuration
    config: DacpServiceConfig,
    /// Whether service is registered
    registered: bool,
}

impl DacpService {
    /// Create new DACP service
    pub fn new(config: DacpServiceConfig) -> Self {
        Self {
            config,
            registered: false,
        }
    }

    /// Get configuration
    pub fn config(&self) -> &DacpServiceConfig {
        &self.config
    }

    /// Get DACP-ID header value
    pub fn dacp_id(&self) -> &str {
        &self.config.dacp_id
    }

    /// Get Active-Remote header value
    pub fn active_remote(&self) -> &str {
        &self.config.active_remote
    }

    /// Register service with mDNS
    pub async fn register(&mut self) -> Result<(), DacpError> {
        // Use mdns-sd to register service
        // Implementation depends on mDNS library

        self.registered = true;
        Ok(())
    }

    /// Unregister service
    pub async fn unregister(&mut self) -> Result<(), DacpError> {
        self.registered = false;
        Ok(())
    }

    /// Check if service is registered
    pub fn is_registered(&self) -> bool {
        self.registered
    }
}

/// DACP errors
#[derive(Debug, thiserror::Error)]
pub enum DacpError {
    #[error("service registration failed: {0}")]
    RegistrationFailed(String),
    #[error("service not registered")]
    NotRegistered,
    #[error("invalid command")]
    InvalidCommand,
    #[error("authentication failed")]
    AuthenticationFailed,
}
```

---

### 30.3 DACP HTTP Server

- [ ] **30.3.1** Implement HTTP server for DACP commands

**File:** `src/protocol/dacp/server.rs`

```rust
//! DACP HTTP server for receiving commands

use super::commands::{DacpCommand, CommandResult};
use super::service::DacpError;
use std::sync::Arc;

/// Handler trait for DACP commands
pub trait DacpHandler: Send + Sync {
    /// Handle incoming command
    fn handle_command(&self, command: DacpCommand) -> CommandResult;

    /// Verify Active-Remote token
    fn verify_token(&self, token: &str) -> bool;
}

/// DACP HTTP server
pub struct DacpServer<H: DacpHandler> {
    /// Handler for commands
    handler: Arc<H>,
    /// Expected Active-Remote token
    expected_token: String,
    /// Server port
    port: u16,
    /// Running state
    running: bool,
}

impl<H: DacpHandler + 'static> DacpServer<H> {
    /// Create new DACP server
    pub fn new(handler: H, expected_token: String, port: u16) -> Self {
        Self {
            handler: Arc::new(handler),
            expected_token,
            port,
            running: false,
        }
    }

    /// Start the HTTP server
    pub async fn start(&mut self) -> Result<(), DacpError> {
        // HTTP server implementation using hyper or similar
        // Listen for GET requests on /ctrl-int/1/*

        self.running = true;

        // Pseudo-code for request handling:
        // loop {
        //     let request = accept_connection().await;
        //     let token = request.headers.get("Active-Remote");
        //
        //     if !self.handler.verify_token(token) {
        //         respond_403();
        //         continue;
        //     }
        //
        //     let command = DacpCommand::from_path(request.path());
        //     let result = self.handler.handle_command(command);
        //
        //     respond_204();
        // }

        Ok(())
    }

    /// Stop the server
    pub async fn stop(&mut self) {
        self.running = false;
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Process an HTTP request (for testing)
    pub fn process_request(
        &self,
        method: &str,
        path: &str,
        active_remote: Option<&str>,
    ) -> HttpResponse {
        // Only accept GET requests
        if method != "GET" {
            return HttpResponse::method_not_allowed();
        }

        // Verify Active-Remote token
        match active_remote {
            Some(token) if self.handler.verify_token(token) => {}
            _ => return HttpResponse::forbidden(),
        }

        // Parse command
        let command = match DacpCommand::from_path(path) {
            Some(cmd) => cmd,
            None => return HttpResponse::not_found(),
        };

        // Execute command
        let result = self.handler.handle_command(command);

        match result {
            CommandResult::Success => HttpResponse::no_content(),
            CommandResult::NotSupported => HttpResponse::not_implemented(),
            CommandResult::Failed(_) => HttpResponse::internal_error(),
        }
    }
}

/// Simple HTTP response representation
pub struct HttpResponse {
    pub status: u16,
    pub reason: &'static str,
}

impl HttpResponse {
    pub fn no_content() -> Self {
        Self { status: 204, reason: "No Content" }
    }

    pub fn forbidden() -> Self {
        Self { status: 403, reason: "Forbidden" }
    }

    pub fn not_found() -> Self {
        Self { status: 404, reason: "Not Found" }
    }

    pub fn method_not_allowed() -> Self {
        Self { status: 405, reason: "Method Not Allowed" }
    }

    pub fn not_implemented() -> Self {
        Self { status: 501, reason: "Not Implemented" }
    }

    pub fn internal_error() -> Self {
        Self { status: 500, reason: "Internal Server Error" }
    }
}

/// Default command handler that forwards to callbacks
pub struct CallbackHandler {
    token: String,
    on_play: Option<Box<dyn Fn() + Send + Sync>>,
    on_pause: Option<Box<dyn Fn() + Send + Sync>>,
    on_next: Option<Box<dyn Fn() + Send + Sync>>,
    on_previous: Option<Box<dyn Fn() + Send + Sync>>,
    on_volume_up: Option<Box<dyn Fn() + Send + Sync>>,
    on_volume_down: Option<Box<dyn Fn() + Send + Sync>>,
}

impl CallbackHandler {
    /// Create new callback handler
    pub fn new(token: String) -> Self {
        Self {
            token,
            on_play: None,
            on_pause: None,
            on_next: None,
            on_previous: None,
            on_volume_up: None,
            on_volume_down: None,
        }
    }

    /// Set play callback
    pub fn on_play<F: Fn() + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_play = Some(Box::new(f));
        self
    }

    /// Set pause callback
    pub fn on_pause<F: Fn() + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_pause = Some(Box::new(f));
        self
    }

    /// Set next track callback
    pub fn on_next<F: Fn() + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_next = Some(Box::new(f));
        self
    }

    /// Set previous track callback
    pub fn on_previous<F: Fn() + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_previous = Some(Box::new(f));
        self
    }
}

impl DacpHandler for CallbackHandler {
    fn handle_command(&self, command: DacpCommand) -> CommandResult {
        match command {
            DacpCommand::Play | DacpCommand::PlayResume | DacpCommand::PlayResume2 => {
                if let Some(ref f) = self.on_play {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::Pause => {
                if let Some(ref f) = self.on_pause {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::PlayPause => {
                // Toggle - implementation depends on current state
                CommandResult::Success
            }
            DacpCommand::NextItem => {
                if let Some(ref f) = self.on_next {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::PrevItem => {
                if let Some(ref f) = self.on_previous {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::VolumeUp => {
                if let Some(ref f) = self.on_volume_up {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::VolumeDown => {
                if let Some(ref f) = self.on_volume_down {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            _ => CommandResult::NotSupported,
        }
    }

    fn verify_token(&self, token: &str) -> bool {
        token == self.token
    }
}
```

---

## Unit Tests

### Test File: `src/protocol/dacp/commands.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_from_path() {
        assert_eq!(
            DacpCommand::from_path("/ctrl-int/1/play"),
            Some(DacpCommand::Play)
        );
        assert_eq!(
            DacpCommand::from_path("/ctrl-int/1/playpause"),
            Some(DacpCommand::PlayPause)
        );
        assert_eq!(
            DacpCommand::from_path("/ctrl-int/1/nextitem"),
            Some(DacpCommand::NextItem)
        );
        assert_eq!(DacpCommand::from_path("/invalid"), None);
        assert_eq!(DacpCommand::from_path("/ctrl-int/1/unknown"), None);
    }

    #[test]
    fn test_command_path_roundtrip() {
        let commands = [
            DacpCommand::Play,
            DacpCommand::Pause,
            DacpCommand::NextItem,
            DacpCommand::VolumeUp,
        ];

        for cmd in commands {
            let path = cmd.path();
            let parsed = DacpCommand::from_path(path);
            assert_eq!(parsed, Some(cmd));
        }
    }
}
```

### Test File: `src/protocol/dacp/server.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestHandler {
        token: String,
        play_called: AtomicBool,
    }

    impl DacpHandler for TestHandler {
        fn handle_command(&self, command: DacpCommand) -> CommandResult {
            match command {
                DacpCommand::Play => {
                    self.play_called.store(true, Ordering::SeqCst);
                    CommandResult::Success
                }
                _ => CommandResult::NotSupported,
            }
        }

        fn verify_token(&self, token: &str) -> bool {
            token == self.token
        }
    }

    #[test]
    fn test_process_request_success() {
        let handler = TestHandler {
            token: "12345".to_string(),
            play_called: AtomicBool::new(false),
        };

        let server = DacpServer::new(handler, "12345".to_string(), 3689);

        let response = server.process_request(
            "GET",
            "/ctrl-int/1/play",
            Some("12345"),
        );

        assert_eq!(response.status, 204);
    }

    #[test]
    fn test_process_request_bad_token() {
        let handler = TestHandler {
            token: "12345".to_string(),
            play_called: AtomicBool::new(false),
        };

        let server = DacpServer::new(handler, "12345".to_string(), 3689);

        let response = server.process_request(
            "GET",
            "/ctrl-int/1/play",
            Some("wrong"),
        );

        assert_eq!(response.status, 403);
    }

    #[test]
    fn test_process_request_unknown_command() {
        let handler = TestHandler {
            token: "12345".to_string(),
            play_called: AtomicBool::new(false),
        };

        let server = DacpServer::new(handler, "12345".to_string(), 3689);

        let response = server.process_request(
            "GET",
            "/ctrl-int/1/unknown",
            Some("12345"),
        );

        assert_eq!(response.status, 404);
    }

    #[test]
    fn test_service_config() {
        let config = DacpServiceConfig::new();

        assert!(!config.dacp_id.is_empty());
        assert!(!config.active_remote.is_empty());
        assert!(config.instance_name().starts_with("iTunes_Ctrl_"));
    }
}
```

---

## Acceptance Criteria

- [ ] DACP service can be advertised via mDNS
- [ ] All playback commands are recognized
- [ ] Active-Remote token verification works
- [ ] HTTP server responds with correct status codes
- [ ] Callback handlers execute correctly
- [ ] Service registration and unregistration works
- [ ] All unit tests pass
- [ ] Integration tests pass

---

## Notes

- DACP server should run on a separate port from RTSP
- Some receivers may not use DACP (manual control only)
- Consider implementing persistent connection for faster response
- Active-Remote token should be included in all RTSP requests
- DACP is optional - streaming works without it
