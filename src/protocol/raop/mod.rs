//! RAOP (`AirPlay` 1) protocol implementation

mod auth;
mod key_exchange;

#[cfg(test)]
mod tests;

pub use auth::{
    AuthState, CHALLENGE_SIZE, RaopAuthenticator, build_response_message, decode_challenge,
    encode_challenge, generate_challenge, generate_response, verify_response,
};

pub use key_exchange::{AES_IV_SIZE, AES_KEY_SIZE, RaopSessionKeys, parse_session_keys};
