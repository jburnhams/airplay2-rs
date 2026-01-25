//! RAOP (`AirPlay` 1) protocol implementation

mod auth;
pub mod encryption;
mod key_exchange;
pub mod session;

#[cfg(test)]
mod encryption_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tests;

pub use auth::{
    AuthState, CHALLENGE_SIZE, RaopAuthenticator, build_response_message, decode_challenge,
    encode_challenge, generate_challenge, generate_response, verify_response,
};

pub use key_exchange::{AES_IV_SIZE, AES_KEY_SIZE, RaopSessionKeys, parse_session_keys};
pub use session::{RaopRtspSession, RaopSessionState};
