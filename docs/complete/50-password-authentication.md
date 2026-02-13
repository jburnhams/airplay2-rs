# Section 50: Password Authentication Fallback

## Dependencies
- **Section 49**: HomeKit Pairing (Server-Side)
- **Section 47**: AirPlay 2 Service Advertisement (status flags)
- **Section 48**: RTSP/HTTP Server Extensions

## Overview

AirPlay 2 supports password-based authentication as an alternative to HomeKit pairing. When a receiver advertises the password flag (`flags` bit 4), senders will prompt users for a password before connecting.

This provides:
- Simpler authentication for non-HomeKit environments
- Backward compatibility with older clients
- Alternative when HomeKit pairing fails

The password authentication uses the same SRP-6a protocol as HomeKit pairing, but with a user-configured password instead of a displayed PIN.

### Authentication Flow

```
Sender                              Receiver
  │                                    │
  │  (discovers device with pw flag)   │
  │                                    │
  │◀── User prompted for password ────│
  │                                    │
  │─── POST /pair-setup (M1) ─────────▶│
  │                                    │
  │    (same SRP flow as HomeKit)      │
  │                                    │
  │◀── M4 (success or error) ─────────│
```

## Objectives

- Support password-based authentication alongside HomeKit pairing
- Configure password via receiver configuration
- Advertise password requirement in mDNS TXT record
- Integrate seamlessly with existing pairing infrastructure
- Handle password change during runtime

---

## Tasks

### 50.1 Password Configuration

- [x] **50.1.1** Extend configuration for password authentication

**File:** `src/receiver/ap2/config.rs` (additions)

```rust
impl Ap2Config {
    /// Check if password authentication is enabled
    pub fn has_password(&self) -> bool {
        self.password.as_ref().map(|p| !p.is_empty()).unwrap_or(false)
    }

    /// Validate password requirements
    pub fn validate_password(password: &str) -> Result<(), PasswordValidationError> {
        if password.is_empty() {
            return Err(PasswordValidationError::Empty);
        }

        if password.len() < 4 {
            return Err(PasswordValidationError::TooShort { min: 4 });
        }

        if password.len() > 64 {
            return Err(PasswordValidationError::TooLong { max: 64 });
        }

        // Check for problematic characters
        if password.contains('\0') {
            return Err(PasswordValidationError::InvalidCharacter('\0'));
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PasswordValidationError {
    #[error("Password cannot be empty")]
    Empty,

    #[error("Password too short (minimum {min} characters)")]
    TooShort { min: usize },

    #[error("Password too long (maximum {max} characters)")]
    TooLong { max: usize },

    #[error("Password contains invalid character: {0:?}")]
    InvalidCharacter(char),
}
```

---

### 50.2 Password Authentication Handler

- [x] **50.2.1** Implement password-based pairing handler

**File:** `src/receiver/ap2/password_auth.rs`

```rust
//! Password-based Authentication for AirPlay 2
//!
//! This module provides password authentication as an alternative to
//! HomeKit PIN-based pairing. It uses the same SRP-6a protocol but
//! with a user-configured password.

use super::pairing_server::{PairingServer, PairingError, EncryptionKeys};
use super::config::Ap2Config;
use crate::protocol::crypto::ed25519::Ed25519Keypair;
use std::sync::{Arc, RwLock};

/// Password authentication manager
///
/// Wraps the pairing server with password-specific functionality.
pub struct PasswordAuthManager {
    /// Underlying pairing server
    pairing_server: Arc<RwLock<PairingServer>>,

    /// Current password
    password: Arc<RwLock<Option<String>>>,

    /// Authentication enabled flag
    enabled: Arc<RwLock<bool>>,

    /// Failed attempt tracking
    failed_attempts: Arc<RwLock<FailedAttemptTracker>>,
}

/// Track failed authentication attempts for rate limiting
struct FailedAttemptTracker {
    attempts: Vec<std::time::Instant>,
    max_attempts: usize,
    window: std::time::Duration,
    lockout_duration: std::time::Duration,
    locked_until: Option<std::time::Instant>,
}

impl FailedAttemptTracker {
    fn new() -> Self {
        Self {
            attempts: Vec::new(),
            max_attempts: 5,
            window: std::time::Duration::from_secs(60),
            lockout_duration: std::time::Duration::from_secs(300),
            locked_until: None,
        }
    }

    fn is_locked(&self) -> bool {
        if let Some(until) = self.locked_until {
            std::time::Instant::now() < until
        } else {
            false
        }
    }

    fn lockout_remaining(&self) -> Option<std::time::Duration> {
        self.locked_until.and_then(|until| {
            let now = std::time::Instant::now();
            if now < until {
                Some(until - now)
            } else {
                None
            }
        })
    }

    fn record_attempt(&mut self, success: bool) {
        let now = std::time::Instant::now();

        // Clear lockout if expired
        if let Some(until) = self.locked_until {
            if now >= until {
                self.locked_until = None;
                self.attempts.clear();
            }
        }

        if success {
            // Clear failed attempts on success
            self.attempts.clear();
            self.locked_until = None;
        } else {
            // Record failed attempt
            self.attempts.push(now);

            // Remove old attempts outside window
            let window_start = now - self.window;
            self.attempts.retain(|&t| t > window_start);

            // Check if we should lock
            if self.attempts.len() >= self.max_attempts {
                self.locked_until = Some(now + self.lockout_duration);
                log::warn!(
                    "Too many failed password attempts, locked for {:?}",
                    self.lockout_duration
                );
            }
        }
    }
}

impl PasswordAuthManager {
    /// Create a new password auth manager
    pub fn new(identity: Ed25519Keypair) -> Self {
        let pairing_server = PairingServer::new(identity);

        Self {
            pairing_server: Arc::new(RwLock::new(pairing_server)),
            password: Arc::new(RwLock::new(None)),
            enabled: Arc::new(RwLock::new(false)),
            failed_attempts: Arc::new(RwLock::new(FailedAttemptTracker::new())),
        }
    }

    /// Create from configuration
    pub fn from_config(config: &Ap2Config, identity: Ed25519Keypair) -> Self {
        let mut manager = Self::new(identity);

        if let Some(ref password) = config.password {
            manager.set_password(password.clone());
        }

        manager
    }

    /// Set the authentication password
    pub fn set_password(&mut self, password: String) {
        // Validate password
        if let Err(e) = Ap2Config::validate_password(&password) {
            log::warn!("Invalid password: {}", e);
            return;
        }

        // Update pairing server
        {
            let mut server = self.pairing_server.write().unwrap();
            server.set_password(&password);
        }

        // Store password
        *self.password.write().unwrap() = Some(password);
        *self.enabled.write().unwrap() = true;

        log::info!("Password authentication enabled");
    }

    /// Clear the password (disable password auth)
    pub fn clear_password(&mut self) {
        *self.password.write().unwrap() = None;
        *self.enabled.write().unwrap() = false;

        // Reset pairing server
        self.pairing_server.write().unwrap().reset();

        log::info!("Password authentication disabled");
    }

    /// Check if password authentication is enabled
    pub fn is_enabled(&self) -> bool {
        *self.enabled.read().unwrap()
    }

    /// Check if currently locked out due to failed attempts
    pub fn is_locked_out(&self) -> bool {
        self.failed_attempts.read().unwrap().is_locked()
    }

    /// Get remaining lockout duration
    pub fn lockout_remaining(&self) -> Option<std::time::Duration> {
        self.failed_attempts.read().unwrap().lockout_remaining()
    }

    /// Process pair-setup request
    pub fn process_pair_setup(&self, data: &[u8]) -> Result<PairingResponse, PasswordAuthError> {
        // Check lockout
        if self.is_locked_out() {
            let remaining = self.lockout_remaining().unwrap_or_default();
            return Err(PasswordAuthError::LockedOut {
                remaining_seconds: remaining.as_secs() as u32,
            });
        }

        // Check if enabled
        if !self.is_enabled() {
            return Err(PasswordAuthError::NotEnabled);
        }

        // Process through pairing server
        let result = self.pairing_server.write().unwrap().process_pair_setup(data);

        // Track success/failure
        let success = result.error.is_none();
        let is_m4 = result.new_state == super::pairing_server::PairingServerState::PairSetupComplete;

        if is_m4 {
            self.failed_attempts.write().unwrap().record_attempt(success);
        }

        if let Some(ref error) = result.error {
            // Check for authentication failure specifically
            if matches!(error, PairingError::AuthenticationFailed) {
                log::warn!("Password authentication failed");
            }
        }

        Ok(PairingResponse {
            data: result.response,
            complete: result.complete,
            error: result.error.map(|e| e.to_string()),
        })
    }

    /// Process pair-verify request
    pub fn process_pair_verify(&self, data: &[u8]) -> Result<PairingResponse, PasswordAuthError> {
        if self.is_locked_out() {
            let remaining = self.lockout_remaining().unwrap_or_default();
            return Err(PasswordAuthError::LockedOut {
                remaining_seconds: remaining.as_secs() as u32,
            });
        }

        let result = self.pairing_server.write().unwrap().process_pair_verify(data);

        Ok(PairingResponse {
            data: result.response,
            complete: result.complete,
            error: result.error.map(|e| e.to_string()),
        })
    }

    /// Get encryption keys after successful pairing
    pub fn encryption_keys(&self) -> Option<EncryptionKeys> {
        self.pairing_server.read().unwrap().encryption_keys().cloned()
    }

    /// Reset for new authentication attempt
    pub fn reset(&self) {
        self.pairing_server.write().unwrap().reset();
    }
}

/// Response from pairing operation
#[derive(Debug)]
pub struct PairingResponse {
    /// Response data to send
    pub data: Vec<u8>,
    /// Whether pairing is complete
    pub complete: bool,
    /// Error message if any
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PasswordAuthError {
    #[error("Password authentication not enabled")]
    NotEnabled,

    #[error("Too many failed attempts, locked out for {remaining_seconds} seconds")]
    LockedOut { remaining_seconds: u32 },

    #[error("Pairing error: {0}")]
    PairingError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_validation() {
        // Valid passwords
        assert!(Ap2Config::validate_password("1234").is_ok());
        assert!(Ap2Config::validate_password("password123").is_ok());

        // Invalid passwords
        assert!(Ap2Config::validate_password("").is_err());
        assert!(Ap2Config::validate_password("123").is_err()); // Too short
    }

    #[test]
    fn test_lockout_tracking() {
        let mut tracker = FailedAttemptTracker::new();
        tracker.max_attempts = 3;
        tracker.window = std::time::Duration::from_secs(60);
        tracker.lockout_duration = std::time::Duration::from_secs(5);

        // First few attempts should not lock
        tracker.record_attempt(false);
        assert!(!tracker.is_locked());
        tracker.record_attempt(false);
        assert!(!tracker.is_locked());

        // Third attempt should lock
        tracker.record_attempt(false);
        assert!(tracker.is_locked());
        assert!(tracker.lockout_remaining().is_some());
    }

    #[test]
    fn test_successful_auth_clears_attempts() {
        let mut tracker = FailedAttemptTracker::new();

        tracker.record_attempt(false);
        tracker.record_attempt(false);
        assert_eq!(tracker.attempts.len(), 2);

        // Successful attempt clears history
        tracker.record_attempt(true);
        assert_eq!(tracker.attempts.len(), 0);
        assert!(!tracker.is_locked());
    }

    #[test]
    fn test_manager_creation() {
        let identity = Ed25519Keypair::generate();
        let manager = PasswordAuthManager::new(identity);

        assert!(!manager.is_enabled());
        assert!(!manager.is_locked_out());
    }

    #[test]
    fn test_set_password_enables_auth() {
        let identity = Ed25519Keypair::generate();
        let mut manager = PasswordAuthManager::new(identity);

        manager.set_password("test1234".to_string());
        assert!(manager.is_enabled());

        manager.clear_password();
        assert!(!manager.is_enabled());
    }
}
```

---

### 50.3 Status Flag Updates

- [x] **50.3.1** Ensure mDNS advertisement reflects password status

**File:** `src/receiver/ap2/advertisement.rs` (additions)

```rust
impl Ap2TxtRecord {
    /// Update password status in TXT record
    pub fn update_password_status(&mut self, has_password: bool) {
        // Update status flags
        let mut status_flags = self.get(txt_keys::STATUS_FLAGS)
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0);

        if has_password {
            status_flags |= 1 << 4;  // Set password required flag
            status_flags |= 1 << 5;  // Set password configured flag
        } else {
            status_flags &= !(1 << 4);  // Clear password required
            status_flags &= !(1 << 5);  // Clear password configured
        }

        self.set(txt_keys::STATUS_FLAGS, format!("0x{:X}", status_flags));
    }
}
```

---

### 50.4 Integration with Request Handler

- [x] **50.4.1** Wire password auth into request handling

**File:** `src/receiver/ap2/password_integration.rs`

```rust
//! Integration of password authentication with request handling

use super::password_auth::{PasswordAuthManager, PasswordAuthError};
use super::pairing_handlers::PairingHandler;
use super::request_handler::{Ap2HandleResult, Ap2RequestContext};
use super::response_builder::Ap2ResponseBuilder;
use crate::protocol::rtsp::{RtspRequest, StatusCode};
use crate::protocol::pairing::tlv::TlvEncoder;

/// Authentication mode for the receiver
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// No authentication required
    None,
    /// Password authentication
    Password,
    /// HomeKit PIN pairing
    HomeKitPin,
    /// Both password and HomeKit supported
    Both,
}

/// Combined authentication handler
pub struct AuthenticationHandler {
    /// Password authentication manager
    password_auth: Option<PasswordAuthManager>,

    /// HomeKit pairing handler
    homekit_handler: Option<PairingHandler>,

    /// Current authentication mode
    mode: AuthMode,
}

impl AuthenticationHandler {
    /// Create handler with password authentication only
    pub fn password_only(manager: PasswordAuthManager) -> Self {
        Self {
            password_auth: Some(manager),
            homekit_handler: None,
            mode: AuthMode::Password,
        }
    }

    /// Create handler with HomeKit pairing only
    pub fn homekit_only(handler: PairingHandler) -> Self {
        Self {
            password_auth: None,
            homekit_handler: Some(handler),
            mode: AuthMode::HomeKitPin,
        }
    }

    /// Create handler supporting both authentication methods
    pub fn both(
        password_manager: PasswordAuthManager,
        homekit_handler: PairingHandler,
    ) -> Self {
        Self {
            password_auth: Some(password_manager),
            homekit_handler: Some(homekit_handler),
            mode: AuthMode::Both,
        }
    }

    /// Handle pair-setup request
    pub fn handle_pair_setup(
        &self,
        request: &RtspRequest,
        cseq: u32,
        _context: &Ap2RequestContext,
    ) -> Ap2HandleResult {
        // Check for lockout
        if let Some(ref pw_auth) = self.password_auth {
            if pw_auth.is_locked_out() {
                return self.lockout_response(cseq, pw_auth);
            }
        }

        // Try password auth first if enabled
        if let Some(ref pw_auth) = self.password_auth {
            if pw_auth.is_enabled() {
                match pw_auth.process_pair_setup(&request.body) {
                    Ok(response) => {
                        return self.pairing_response_to_handle_result(response, cseq);
                    }
                    Err(PasswordAuthError::NotEnabled) => {
                        // Fall through to HomeKit
                    }
                    Err(e) => {
                        return self.error_response(cseq, &e.to_string());
                    }
                }
            }
        }

        // Try HomeKit pairing
        if let Some(ref hk_handler) = self.homekit_handler {
            return hk_handler.handle_pair_setup(request, cseq);
        }

        // No authentication configured
        self.error_response(cseq, "No authentication method available")
    }

    /// Handle pair-verify request
    pub fn handle_pair_verify(
        &self,
        request: &RtspRequest,
        cseq: u32,
        _context: &Ap2RequestContext,
    ) -> Ap2HandleResult {
        // Try password auth
        if let Some(ref pw_auth) = self.password_auth {
            if pw_auth.is_enabled() {
                match pw_auth.process_pair_verify(&request.body) {
                    Ok(response) => {
                        return self.pairing_response_to_handle_result(response, cseq);
                    }
                    Err(PasswordAuthError::NotEnabled) => {
                        // Fall through
                    }
                    Err(e) => {
                        return self.error_response(cseq, &e.to_string());
                    }
                }
            }
        }

        // Try HomeKit
        if let Some(ref hk_handler) = self.homekit_handler {
            return hk_handler.handle_pair_verify(request, cseq);
        }

        self.error_response(cseq, "No authentication method available")
    }

    fn lockout_response(&self, cseq: u32, pw_auth: &PasswordAuthManager) -> Ap2HandleResult {
        let remaining = pw_auth.lockout_remaining()
            .map(|d| d.as_secs() as u32)
            .unwrap_or(300);

        // Build TLV with retry delay
        let response_tlv = TlvEncoder::new()
            .add_u8(0x06, 0)  // State = error
            .add_u8(0x07, 0x03)  // Error = backoff
            .add_bytes(0x08, &remaining.to_le_bytes())  // Retry delay
            .encode();

        Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode(503))
                .cseq(cseq)
                .header("Retry-After", &remaining.to_string())
                .binary_body(response_tlv)
                .encode(),
            new_state: None,
            event: None,
            error: Some(format!("Locked out for {} seconds", remaining)),
        }
    }

    fn pairing_response_to_handle_result(
        &self,
        response: super::password_auth::PairingResponse,
        cseq: u32,
    ) -> Ap2HandleResult {
        use super::session_state::Ap2SessionState;

        let (status, new_state) = if response.error.is_some() {
            (StatusCode::CONNECTION_AUTH_REQUIRED, Some(Ap2SessionState::Error {
                code: 470,
                message: response.error.clone().unwrap_or_default(),
            }))
        } else if response.complete {
            (StatusCode::OK, Some(Ap2SessionState::Paired))
        } else {
            (StatusCode::OK, None)
        };

        Ap2HandleResult {
            response: Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .binary_body(response.data)
                .encode(),
            new_state,
            event: if response.complete {
                Some(super::request_handler::Ap2Event::PairingComplete {
                    session_key: vec![], // Filled by actual handler
                })
            } else {
                None
            },
            error: response.error,
        }
    }

    fn error_response(&self, cseq: u32, message: &str) -> Ap2HandleResult {
        Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode::NOT_IMPLEMENTED)
                .cseq(cseq)
                .encode(),
            new_state: None,
            event: None,
            error: Some(message.to_string()),
        }
    }
}
```

---

## Unit Tests

### 50.5 Password Authentication Tests

- [x] **50.5.1** Test password configuration and validation

**File:** `tests/receiver/password_auth_tests.rs`

```rust
use airplay2::receiver::ap2::password_auth::PasswordAuthManager;
use airplay2::receiver::ap2::config::Ap2Config;
use airplay2::protocol::crypto::ed25519::Ed25519Keypair;

#[test]
fn test_password_manager_lifecycle() {
    let identity = Ed25519Keypair::generate();
    let mut manager = PasswordAuthManager::new(identity);

    // Initially disabled
    assert!(!manager.is_enabled());

    // Enable with password
    manager.set_password("secret123".to_string());
    assert!(manager.is_enabled());

    // Disable
    manager.clear_password();
    assert!(!manager.is_enabled());
}

#[test]
fn test_password_validation_rules() {
    // Too short
    assert!(Ap2Config::validate_password("abc").is_err());

    // Empty
    assert!(Ap2Config::validate_password("").is_err());

    // Valid minimum length
    assert!(Ap2Config::validate_password("abcd").is_ok());

    // Valid longer password
    assert!(Ap2Config::validate_password("my_secure_password_123").is_ok());
}

#[test]
fn test_lockout_disabled_by_default() {
    let identity = Ed25519Keypair::generate();
    let manager = PasswordAuthManager::new(identity);

    assert!(!manager.is_locked_out());
    assert!(manager.lockout_remaining().is_none());
}
```

---

## Acceptance Criteria

- [x] Password can be configured and validated
- [x] Password authentication uses SRP-6a protocol
- [x] mDNS TXT record reflects password requirement
- [x] Failed attempts are tracked
- [x] Lockout activates after max failed attempts
- [x] Lockout duration is enforced
- [x] Successful authentication clears lockout
- [x] Password change takes effect immediately
- [x] Both password and HomeKit auth can coexist
- [x] All unit tests pass

---

## Notes

### Security Considerations

- Passwords should be stored securely (hashed if persistent)
- Rate limiting prevents brute force attacks
- Lockout provides defense against automated attacks
- Password transmission is protected by SRP-6a (password never sent)

### User Experience

- When password is configured, senders display a password prompt
- Failed password shows generic error (doesn't reveal if password exists)
- Lockout duration should be balanced (security vs. usability)

---

## References

- [SRP-6a Security Analysis](http://srp.stanford.edu/ndss.html)
- [OWASP Password Storage](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- [Section 49: HomeKit Pairing](./49-homekit-pairing-server.md)
