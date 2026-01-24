//! RAOP challenge-response authentication

use super::super::crypto::{AppleRsaPublicKey, RaopRsaPrivateKey};
use super::super::crypto::CryptoError;
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD as BASE64};

/// Challenge size in bytes (128 bits)
pub const CHALLENGE_SIZE: usize = 16;

/// Generate a random Apple-Challenge
#[must_use]
pub fn generate_challenge() -> [u8; CHALLENGE_SIZE] {
    use rand::RngCore;

    let mut challenge = [0u8; CHALLENGE_SIZE];
    rand::thread_rng().fill_bytes(&mut challenge);
    challenge
}

/// Encode challenge as Base64 for Apple-Challenge header
#[must_use]
pub fn encode_challenge(challenge: &[u8]) -> String {
    BASE64.encode(challenge)
}

/// Decode challenge from Apple-Challenge header
///
/// # Errors
///
/// Returns `CryptoError::DecryptionFailed` if the input is not valid base64.
pub fn decode_challenge(header: &str) -> Result<Vec<u8>, CryptoError> {
    BASE64.decode(header.trim())
        .map_err(|e| CryptoError::DecryptionFailed(format!("invalid base64: {e}")))
}

/// Build the message to sign for Apple-Response
///
/// The response is: `RSA_Sign(challenge` || `ip_address` || `mac_address`)
#[must_use]
pub fn build_response_message(
    challenge: &[u8],
    ip_address: &std::net::IpAddr,
    mac_address: &[u8; 6],
) -> Vec<u8> {
    let mut message = Vec::with_capacity(CHALLENGE_SIZE + 16 + 6);

    // Add challenge
    message.extend_from_slice(challenge);

    // Add IP address (4 bytes for IPv4, 16 for IPv6)
    match ip_address {
        std::net::IpAddr::V4(addr) => {
            message.extend_from_slice(&addr.octets());
        }
        std::net::IpAddr::V6(addr) => {
            message.extend_from_slice(&addr.octets());
        }
    }

    // Add MAC address
    message.extend_from_slice(mac_address);

    // Pad to 32 bytes if needed (some implementations require this)
    while message.len() < 32 {
        message.push(0);
    }

    message
}

/// Generate Apple-Response for a given challenge (server-side)
///
/// # Errors
///
/// Returns `CryptoError` if signing fails.
pub fn generate_response(
    private_key: &RaopRsaPrivateKey,
    challenge: &[u8],
    ip_address: &std::net::IpAddr,
    mac_address: &[u8; 6],
) -> Result<String, CryptoError> {
    let message = build_response_message(challenge, ip_address, mac_address);
    let signature = private_key.sign_pkcs1(&message)?;
    Ok(BASE64.encode(&signature))
}

/// Verify Apple-Response header (client-side)
///
/// # Errors
///
/// Returns `CryptoError` if verification fails or header is invalid base64.
pub fn verify_response(
    public_key: &AppleRsaPublicKey,
    response_header: &str,
    challenge: &[u8],
    server_ip: &std::net::IpAddr,
    server_mac: &[u8; 6],
) -> Result<(), CryptoError> {
    let signature = BASE64.decode(response_header.trim())
        .map_err(|_| CryptoError::VerificationFailed)?;

    let message = build_response_message(challenge, server_ip, server_mac);
    public_key.verify_pkcs1(&message, &signature)
}

/// RAOP authentication state machine
pub struct RaopAuthenticator {
    /// Generated challenge
    challenge: [u8; CHALLENGE_SIZE],
    /// State of authentication
    state: AuthState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    /// Initial state, challenge not sent
    Initial,
    /// Challenge sent, waiting for response
    ChallengeSent,
    /// Response verified successfully
    Authenticated,
    /// Authentication failed
    Failed,
}

impl Default for RaopAuthenticator {
    fn default() -> Self {
        Self::new()
    }
}

impl RaopAuthenticator {
    /// Create new authenticator
    #[must_use]
    pub fn new() -> Self {
        Self {
            challenge: generate_challenge(),
            state: AuthState::Initial,
        }
    }

    /// Get current state
    #[must_use]
    pub fn state(&self) -> AuthState {
        self.state
    }

    /// Get the Apple-Challenge header value
    #[must_use]
    pub fn challenge_header(&self) -> String {
        encode_challenge(&self.challenge)
    }

    /// Mark challenge as sent
    pub fn mark_sent(&mut self) {
        self.state = AuthState::ChallengeSent;
    }

    /// Verify the Apple-Response header
    ///
    /// # Errors
    ///
    /// Returns `CryptoError` if verification fails or state is invalid.
    pub fn verify(
        &mut self,
        response_header: &str,
        server_ip: &std::net::IpAddr,
        server_mac: &[u8; 6],
    ) -> Result<(), CryptoError> {
        if self.state != AuthState::ChallengeSent {
            return Err(CryptoError::VerificationFailed);
        }

        let public_key = AppleRsaPublicKey::load()?;
        let result = verify_response(
            &public_key,
            response_header,
            &self.challenge,
            server_ip,
            server_mac,
        );

        self.state = if result.is_ok() {
            AuthState::Authenticated
        } else {
            AuthState::Failed
        };

        result
    }

    /// Check if authentication is complete
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.state == AuthState::Authenticated
    }
}
