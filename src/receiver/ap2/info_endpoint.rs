//! GET /info endpoint handler
//!
//! Returns device capabilities to connecting senders.

use super::body_handler::encode_bplist_body;
use super::capabilities::DeviceCapabilities;
use super::request_handler::{Ap2HandleResult, Ap2RequestContext};
use super::response_builder::Ap2ResponseBuilder;
use crate::protocol::rtsp::RtspRequest;
use crate::protocol::rtsp::StatusCode;
use std::sync::Arc;

/// Handler for GET /info endpoint
pub struct InfoEndpoint {
    /// Device capabilities
    capabilities: Arc<DeviceCapabilities>,
}

impl InfoEndpoint {
    /// Create a new info endpoint handler
    #[must_use]
    pub fn new(capabilities: DeviceCapabilities) -> Self {
        Self {
            capabilities: Arc::new(capabilities),
        }
    }

    /// Handle GET /info request
    pub fn handle(
        &self,
        request: &RtspRequest,
        cseq: u32,
        _context: &Ap2RequestContext,
    ) -> Ap2HandleResult {
        tracing::debug!("Handling GET /info request");

        // Check for qualifier header (optional, indicates specific info requested)
        let qualifier = request.headers.get("X-Apple-Info-Qualifier");

        // Generate capabilities plist
        let plist = if let Some(_qualifier) = qualifier {
            // Could filter based on qualifier
            self.capabilities.to_plist()
        } else {
            self.capabilities.to_plist()
        };

        // Encode to binary plist
        let body = match encode_bplist_body(&plist) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("Failed to encode /info response: {e}");
                return Ap2HandleResult {
                    response: Ap2ResponseBuilder::error(StatusCode::INTERNAL_ERROR)
                        .cseq(cseq)
                        .encode(),
                    new_state: None,
                    event: None,
                    error: Some(format!("Failed to encode response: {e}")),
                };
            }
        };

        tracing::debug!("/info response: {} bytes", body.len());

        Ap2HandleResult {
            response: Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .binary_body(body)
                .header("Content-Type", "application/x-apple-binary-plist")
                .encode(),
            new_state: Some(super::session_state::Ap2SessionState::InfoExchanged),
            event: None,
            error: None,
        }
    }

    /// Update capabilities (e.g., when configuration changes)
    pub fn update_capabilities(&mut self, capabilities: DeviceCapabilities) {
        self.capabilities = Arc::new(capabilities);
    }

    /// Get current capabilities
    #[must_use]
    pub fn capabilities(&self) -> &DeviceCapabilities {
        &self.capabilities
    }
}

/// Create handler function for request router
pub fn create_info_handler(
    endpoint: Arc<InfoEndpoint>,
) -> impl Fn(&RtspRequest, u32, &Ap2RequestContext) -> Ap2HandleResult {
    move |req, cseq, ctx| endpoint.handle(req, cseq, ctx)
}
