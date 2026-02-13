//! Password-based Authentication for `AirPlay` 2
//!
//! This module provides password authentication as an alternative to
//! `HomeKit` PIN-based pairing. It uses the same SRP-6a protocol but
//! with a user-configured password.

use super::config::Ap2Config;
use super::pairing_server::{EncryptionKeys, PairingError, PairingServer, PairingServerState};
use crate::protocol::crypto::Ed25519KeyPair;
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
            if now < until { Some(until - now) } else { None }
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
            let window_start = now
                .checked_sub(self.window)
                .unwrap_or_else(std::time::Instant::now);
            self.attempts.retain(|&t| t > window_start);

            // Check if we should lock
            if self.attempts.len() >= self.max_attempts {
                self.locked_until = Some(now + self.lockout_duration);
                tracing::warn!(
                    "Too many failed password attempts, locked for {:?}",
                    self.lockout_duration
                );
            }
        }
    }
}

impl PasswordAuthManager {
    /// Create a new password auth manager
    #[must_use]
    pub fn new(identity: Ed25519KeyPair) -> Self {
        let pairing_server = PairingServer::new(identity);

        Self {
            pairing_server: Arc::new(RwLock::new(pairing_server)),
            password: Arc::new(RwLock::new(None)),
            enabled: Arc::new(RwLock::new(false)),
            failed_attempts: Arc::new(RwLock::new(FailedAttemptTracker::new())),
        }
    }

    /// Create from configuration
    #[must_use]
    pub fn from_config(config: &Ap2Config, identity: Ed25519KeyPair) -> Self {
        let mut manager = Self::new(identity);

        if let Some(ref password) = config.password {
            manager.set_password(password.clone());
        }

        manager
    }

    /// Set the authentication password
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn set_password(&mut self, password: String) {
        // Validate password
        if let Err(e) = Ap2Config::validate_password(&password) {
            tracing::warn!("Invalid password: {}", e);
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

        tracing::info!("Password authentication enabled");
    }

    /// Clear the password (disable password auth)
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn clear_password(&mut self) {
        *self.password.write().unwrap() = None;
        *self.enabled.write().unwrap() = false;

        // Reset pairing server
        self.pairing_server.write().unwrap().reset();

        tracing::info!("Password authentication disabled");
    }

    /// Check if password authentication is enabled
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        *self.enabled.read().unwrap()
    }

    /// Check if currently locked out due to failed attempts
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn is_locked_out(&self) -> bool {
        self.failed_attempts.read().unwrap().is_locked()
    }

    /// Get remaining lockout duration
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn lockout_remaining(&self) -> Option<std::time::Duration> {
        self.failed_attempts.read().unwrap().lockout_remaining()
    }

    /// Process pair-setup request
    ///
    /// # Errors
    ///
    /// Returns `PasswordAuthError` if processing fails or auth is disabled/locked.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn process_pair_setup(&self, data: &[u8]) -> Result<PairingResponse, PasswordAuthError> {
        // Check lockout
        if self.is_locked_out() {
            let remaining = self.lockout_remaining().unwrap_or_default();
            return Err(PasswordAuthError::LockedOut {
                remaining_seconds: u32::try_from(remaining.as_secs()).unwrap_or(u32::MAX),
            });
        }

        // Check if enabled
        if !self.is_enabled() {
            return Err(PasswordAuthError::NotEnabled);
        }

        // Process through pairing server
        let result = self
            .pairing_server
            .write()
            .unwrap()
            .process_pair_setup(data);

        // Track success/failure
        let success = result.error.is_none();
        let is_m4 = result.new_state == PairingServerState::PairSetupComplete;

        if is_m4 {
            self.failed_attempts
                .write()
                .unwrap()
                .record_attempt(success);
        }

        if let Some(ref error) = result.error {
            // Check for authentication failure specifically
            if matches!(error, PairingError::AuthenticationFailed) {
                tracing::warn!("Password authentication failed");
            }
        }

        Ok(PairingResponse {
            data: result.response,
            complete: result.complete,
            error: result.error.map(|e| e.to_string()),
        })
    }

    /// Process pair-verify request
    ///
    /// # Errors
    ///
    /// Returns `PasswordAuthError` if processing fails or auth is locked.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn process_pair_verify(&self, data: &[u8]) -> Result<PairingResponse, PasswordAuthError> {
        if self.is_locked_out() {
            let remaining = self.lockout_remaining().unwrap_or_default();
            return Err(PasswordAuthError::LockedOut {
                remaining_seconds: u32::try_from(remaining.as_secs()).unwrap_or(u32::MAX),
            });
        }

        let result = self
            .pairing_server
            .write()
            .unwrap()
            .process_pair_verify(data);

        Ok(PairingResponse {
            data: result.response,
            complete: result.complete,
            error: result.error.map(|e| e.to_string()),
        })
    }

    /// Get encryption keys after successful pairing
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn encryption_keys(&self) -> Option<EncryptionKeys> {
        self.pairing_server
            .read()
            .unwrap()
            .encryption_keys()
            .cloned()
    }

    /// Reset for new authentication attempt
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
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

/// Password authentication errors
#[derive(Debug, thiserror::Error)]
pub enum PasswordAuthError {
    /// Password authentication not enabled
    #[error("Password authentication not enabled")]
    NotEnabled,

    /// Locked out due to too many failed attempts
    #[error("Too many failed attempts, locked out for {remaining_seconds} seconds")]
    LockedOut {
        /// Remaining lockout seconds
        remaining_seconds: u32,
    },

    /// Pairing error
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
        let identity = Ed25519KeyPair::generate();
        let manager = PasswordAuthManager::new(identity);

        assert!(!manager.is_enabled());
        assert!(!manager.is_locked_out());
    }

    #[test]
    fn test_set_password_enables_auth() {
        let identity = Ed25519KeyPair::generate();
        let mut manager = PasswordAuthManager::new(identity);

        manager.set_password("test1234".to_string());
        assert!(manager.is_enabled());

        manager.clear_password();
        assert!(!manager.is_enabled());
    }
}
