//! Integration of password authentication with request handling

use super::pairing_handlers::PairingHandler;
use super::password_auth::{PasswordAuthError, PasswordAuthManager};
use super::request_handler::{Ap2Event, Ap2HandleResult, Ap2RequestContext};
use super::response_builder::Ap2ResponseBuilder;
use super::session_state::Ap2SessionState;
use crate::protocol::pairing::tlv::{TlvEncoder, TlvType};
use crate::protocol::rtsp::{RtspRequest, StatusCode};

/// Authentication mode for the receiver
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// No authentication required
    None,
    /// Password authentication
    Password,
    /// `HomeKit` PIN pairing
    HomeKitPin,
    /// Both password and `HomeKit` supported
    Both,
}

/// Combined authentication handler
pub struct AuthenticationHandler {
    /// Password authentication manager
    password_auth: Option<PasswordAuthManager>,

    /// `HomeKit` pairing handler
    homekit_handler: Option<PairingHandler>,

    /// Current authentication mode
    #[allow(dead_code)]
    mode: AuthMode,
}

impl AuthenticationHandler {
    /// Create handler with password authentication only
    #[must_use]
    pub fn password_only(manager: PasswordAuthManager) -> Self {
        Self {
            password_auth: Some(manager),
            homekit_handler: None,
            mode: AuthMode::Password,
        }
    }

    /// Create handler with `HomeKit` pairing only
    #[must_use]
    pub fn homekit_only(handler: PairingHandler) -> Self {
        Self {
            password_auth: None,
            homekit_handler: Some(handler),
            mode: AuthMode::HomeKitPin,
        }
    }

    /// Create handler supporting both authentication methods
    #[must_use]
    pub fn both(password_manager: PasswordAuthManager, homekit_handler: PairingHandler) -> Self {
        Self {
            password_auth: Some(password_manager),
            homekit_handler: Some(homekit_handler),
            mode: AuthMode::Both,
        }
    }

    /// Handle pair-setup request
    #[must_use]
    pub fn handle_pair_setup(
        &self,
        request: &RtspRequest,
        cseq: u32,
        _context: &Ap2RequestContext,
    ) -> Ap2HandleResult {
        // Check for lockout
        if let Some(ref pw_auth) = self.password_auth {
            if pw_auth.is_locked_out() {
                return Self::lockout_response(cseq, pw_auth);
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
                        return Self::error_response(cseq, &e.to_string());
                    }
                }
            }
        }

        // Try HomeKit pairing
        if let Some(ref hk_handler) = self.homekit_handler {
            return hk_handler.handle_pair_setup(request, cseq);
        }

        // No authentication configured
        Self::error_response(cseq, "No authentication method available")
    }

    /// Handle pair-verify request
    #[must_use]
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
                        return Self::error_response(cseq, &e.to_string());
                    }
                }
            }
        }

        // Try HomeKit
        if let Some(ref hk_handler) = self.homekit_handler {
            return hk_handler.handle_pair_verify(request, cseq);
        }

        Self::error_response(cseq, "No authentication method available")
    }

    fn lockout_response(cseq: u32, pw_auth: &PasswordAuthManager) -> Ap2HandleResult {
        let remaining = pw_auth
            .lockout_remaining()
            .map_or(300, |d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX));

        // Build TLV with retry delay
        let response_tlv = TlvEncoder::new()
            .add_byte(TlvType::State, 0) // State = error
            .add_byte(TlvType::Error, 0x03) // Error = backoff
            .add(TlvType::RetryDelay, &remaining.to_le_bytes()) // Retry delay
            .build();

        Ap2HandleResult {
            response: Ap2ResponseBuilder::error(StatusCode(503))
                .cseq(cseq)
                .header("Retry-After", &remaining.to_string())
                .binary_body(response_tlv)
                .encode(),
            new_state: None,
            event: None,
            error: Some(format!("Locked out for {remaining} seconds")),
        }
    }

    fn pairing_response_to_handle_result(
        &self,
        response: super::password_auth::PairingResponse,
        cseq: u32,
    ) -> Ap2HandleResult {
        let new_state = if response.error.is_some() {
            Some(Ap2SessionState::Error {
                code: 470,
                message: response.error.clone().unwrap_or_default(),
            })
        } else if response.complete {
            Some(Ap2SessionState::Paired)
        } else {
            None
        };

        let event = if response.complete {
            let session_key = self
                .password_auth
                .as_ref()
                .and_then(super::password_auth::PasswordAuthManager::encryption_keys)
                .map(|k| k.encrypt_key.to_vec())
                .expect("Encryption keys should be available when pairing is complete");

            Some(Ap2Event::PairingComplete { session_key })
        } else {
            None
        };

        Ap2HandleResult {
            response: Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .binary_body(response.data)
                .encode(),
            new_state,
            event,
            error: response.error,
        }
    }

    fn error_response(cseq: u32, message: &str) -> Ap2HandleResult {
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
