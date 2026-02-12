//! HTTP endpoint handlers for pairing
//!
//! These handlers integrate the `PairingServer` with the RTSP request framework.

use super::pairing_server::{EncryptionKeys, PairingResult, PairingServer, PairingServerState};
use super::request_handler::{Ap2Event, Ap2HandleResult, Ap2RequestContext, HandlerFn};
use super::response_builder::Ap2ResponseBuilder;
use super::session_state::Ap2SessionState;
use crate::protocol::rtsp::{RtspRequest, StatusCode};
use std::sync::{Arc, Mutex};

/// Handler state for pairing operations
pub struct PairingHandler {
    server: Arc<Mutex<PairingServer>>,
}

impl PairingHandler {
    /// Create a new pairing handler wrapper
    #[must_use]
    pub fn new(server: PairingServer) -> Self {
        Self {
            server: Arc::new(Mutex::new(server)),
        }
    }

    /// Handle POST /pair-setup
    ///
    /// # Panics
    ///
    /// Panics if the server lock is poisoned.
    #[must_use]
    pub fn handle_pair_setup(&self, request: &RtspRequest, cseq: u32) -> Ap2HandleResult {
        let mut server = self.server.lock().unwrap();

        // Parse request body (raw TLV, not bplist)
        if request.body.is_empty() {
            return Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::BAD_REQUEST)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: Some("Empty pair-setup body".to_string()),
            };
        }

        let result = server.process_pair_setup(&request.body);

        self.pairing_result_to_handle_result(result, cseq, false)
    }

    /// Handle POST /pair-verify
    ///
    /// # Panics
    ///
    /// Panics if the server lock is poisoned.
    #[must_use]
    pub fn handle_pair_verify(&self, request: &RtspRequest, cseq: u32) -> Ap2HandleResult {
        let mut server = self.server.lock().unwrap();

        if request.body.is_empty() {
            return Ap2HandleResult {
                response: Ap2ResponseBuilder::error(StatusCode::BAD_REQUEST)
                    .cseq(cseq)
                    .encode(),
                new_state: None,
                event: None,
                error: Some("Empty pair-verify body".to_string()),
            };
        }

        let result = server.process_pair_verify(&request.body);

        // Check if pairing is complete
        let is_verify_complete = result.new_state == PairingServerState::Complete;

        self.pairing_result_to_handle_result(result, cseq, is_verify_complete)
    }

    fn pairing_result_to_handle_result(
        &self,
        result: PairingResult,
        cseq: u32,
        emit_complete_event: bool,
    ) -> Ap2HandleResult {
        let new_state = match result.new_state {
            PairingServerState::WaitingForM3 => Some(Ap2SessionState::PairingSetup { step: 2 }),
            PairingServerState::PairSetupComplete => {
                Some(Ap2SessionState::PairingSetup { step: 4 })
            }
            PairingServerState::VerifyWaitingForM3 => {
                Some(Ap2SessionState::PairingVerify { step: 2 })
            }
            PairingServerState::Complete => Some(Ap2SessionState::Paired),
            PairingServerState::Error => Some(Ap2SessionState::Error {
                code: 470,
                message: result.error.as_ref().map_or_else(
                    || "Pairing error".to_string(),
                    std::string::ToString::to_string,
                ),
            }),
            PairingServerState::Idle => None,
        };

        let event = if emit_complete_event && result.complete {
            let server = self.server.lock().unwrap();
            server
                .encryption_keys()
                .map(|keys| Ap2Event::PairingComplete {
                    session_key: keys.encrypt_key.to_vec(),
                })
        } else {
            None
        };

        // Build response with octet-stream content type (raw TLV)
        let response = if result.error.is_some() {
            // Note: Pairing errors often return 200 OK with error TLV,
            // but if something is fundamentally wrong we might send 4xx.
            // HomeKit spec suggests 200 OK with kTLVTypeError unless HTTP error.
            Ap2ResponseBuilder::ok() // changed from CONNECTION_AUTH_REQUIRED based on typical HK behavior
                .cseq(cseq)
                .binary_body(result.response)
                .encode()
        } else {
            Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .binary_body(result.response)
                .encode()
        };

        Ap2HandleResult {
            response,
            new_state,
            event,
            error: result.error.as_ref().map(std::string::ToString::to_string),
        }
    }

    /// Get encryption keys (only valid after successful pairing)
    ///
    /// # Panics
    ///
    /// Panics if the server lock is poisoned.
    #[must_use]
    pub fn encryption_keys(&self) -> Option<EncryptionKeys> {
        self.server.lock().unwrap().encryption_keys().cloned()
    }

    /// Reset for new pairing attempt
    ///
    /// # Panics
    ///
    /// Panics if the server lock is poisoned.
    pub fn reset(&self) {
        self.server.lock().unwrap().reset();
    }
}

/// Create pairing handlers for the request handler framework
#[must_use]
pub fn create_pairing_handlers(handler: Arc<PairingHandler>) -> (HandlerFn, HandlerFn) {
    let setup_handler = handler.clone();
    let verify_handler = handler;

    let pair_setup = Box::new(
        move |req: &RtspRequest, cseq: u32, _ctx: &Ap2RequestContext| {
            setup_handler.handle_pair_setup(req, cseq)
        },
    );

    let pair_verify = Box::new(
        move |req: &RtspRequest, cseq: u32, _ctx: &Ap2RequestContext| {
            verify_handler.handle_pair_verify(req, cseq)
        },
    );

    (pair_setup, pair_verify)
}
