//! Mock `AirPlay` 2 Sender for Testing
//!
//! Simulates an iOS/macOS device connecting to our receiver,
//! performing pairing, and streaming audio.

use std::collections::HashMap;
use std::net::SocketAddr;

use tokio::net::TcpStream;

use crate::net::{AsyncReadExt, AsyncWriteExt};
use crate::protocol::crypto::{Ed25519KeyPair, X25519KeyPair};
use crate::protocol::pairing::tlv::{TlvEncoder, TlvType};
use crate::protocol::plist::PlistValue;
use crate::protocol::rtsp::{Headers, Method, RtspRequest};
use crate::receiver::ap2::body_handler::encode_bplist_body;

/// Mock sender configuration
#[derive(Debug, Clone)]
pub struct MockSenderConfig {
    /// Sender name
    pub name: String,
    /// PIN/password to use for pairing
    pub pin: String,
    /// Audio format to request
    pub audio_format: MockAudioFormat,
    /// Enable encryption
    pub encrypt: bool,
}

/// Audio format to request from the receiver
#[derive(Debug, Clone, Copy)]
pub enum MockAudioFormat {
    /// PCM 44100Hz
    Pcm44100,
    /// ALAC 44100Hz
    Alac44100,
    /// AAC ELD
    AacEld,
}

impl Default for MockSenderConfig {
    fn default() -> Self {
        Self {
            name: "MockSender".to_string(),
            pin: "1234".to_string(),
            audio_format: MockAudioFormat::Pcm44100,
            encrypt: true,
        }
    }
}

/// Mock `AirPlay` 2 sender for testing
#[allow(dead_code)]
pub struct MockAp2Sender {
    config: MockSenderConfig,
    stream: Option<TcpStream>,
    cseq: u32,
    session_id: Option<String>,
    identity: Ed25519KeyPair,
    encryption_key: Option<[u8; 32]>,
}

impl MockAp2Sender {
    /// Create a new mock sender
    #[must_use]
    pub fn new(config: MockSenderConfig) -> Self {
        Self {
            config,
            stream: None,
            cseq: 0,
            session_id: None,
            identity: Ed25519KeyPair::generate(),
            encryption_key: None,
        }
    }

    /// Connect to a receiver
    ///
    /// # Errors
    /// Returns `MockSenderError` if connection fails.
    pub async fn connect(&mut self, addr: SocketAddr) -> Result<(), MockSenderError> {
        self.stream = Some(TcpStream::connect(addr).await?);
        tracing::debug!("Mock sender connected to {}", addr);
        Ok(())
    }

    /// Perform full session: info -> pairing -> setup -> record
    ///
    /// # Errors
    /// Returns `MockSenderError` if any step of the session fails.
    pub async fn full_session(&mut self) -> Result<MockSessionResult, MockSenderError> {
        // Step 1: GET /info
        let _info = self.get_info().await?;
        tracing::debug!("Received device info");

        // Step 2: Pair-setup
        self.pair_setup().await?;
        tracing::debug!("Pairing setup complete");

        // Step 3: Pair-verify
        let encryption_key = self.pair_verify().await?;
        self.encryption_key = Some(encryption_key);
        tracing::debug!("Pairing verify complete, encryption enabled");

        // Step 4: SETUP phase 1 (timing)
        let timing_ports = self.setup_timing().await?;
        tracing::debug!("Setup phase 1 complete: {:?}", timing_ports);

        // Step 5: SETUP phase 2 (audio)
        let audio_ports = self.setup_audio().await?;
        tracing::debug!("Setup phase 2 complete: {:?}", audio_ports);

        // Step 6: RECORD
        self.record().await?;
        tracing::debug!("Recording started");

        Ok(MockSessionResult {
            timing_port: timing_ports.0,
            audio_data_port: audio_ports.0,
            audio_control_port: audio_ports.1,
        })
    }

    /// GET /info request
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn get_info(&mut self) -> Result<PlistValue, MockSenderError> {
        let request = self.build_request(Method::Get, "/info", None);
        let _response = self.send_request(&request).await?;
        // Parse response body as plist
        Ok(PlistValue::Dictionary(HashMap::new())) // Simplified
    }

    /// Perform pair-setup (M1-M4)
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn pair_setup(&mut self) -> Result<(), MockSenderError> {
        // M1: Send method and state
        let mut encoder = TlvEncoder::new();
        encoder = encoder.add(TlvType::State, &[1]); // State = 1
        encoder = encoder.add(TlvType::Method, &[0]); // Method = pair-setup
        let m1 = encoder.build();

        let request = self.build_request(Method::Post, "/pair-setup", Some(m1));
        let _response = self.send_request(&request).await?;

        // Parse M2, compute M3, etc.
        // (Simplified - real implementation would complete SRP)

        Ok(())
    }

    /// Perform pair-verify (M1-M4)
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn pair_verify(&mut self) -> Result<[u8; 32], MockSenderError> {
        let keypair = X25519KeyPair::generate();

        // M1: Send our public key
        let mut encoder = TlvEncoder::new();
        encoder = encoder.add(TlvType::State, &[1]); // State = 1
        encoder = encoder.add(TlvType::PublicKey, keypair.public_key().as_bytes());
        let m1 = encoder.build();

        let request = self.build_request(Method::Post, "/pair-verify", Some(m1));
        let _response = self.send_request(&request).await?;

        // Complete verify exchange...
        // Return derived encryption key
        Ok([0u8; 32]) // Placeholder
    }

    /// SETUP phase 1 (timing)
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn setup_timing(&mut self) -> Result<(u16, u16), MockSenderError> {
        let mut streams = HashMap::new();
        streams.insert("type".to_string(), PlistValue::UnsignedInteger(150)); // Timing

        let body = encode_bplist_body(&PlistValue::Dictionary({
            let mut d = HashMap::new();
            d.insert(
                "streams".to_string(),
                PlistValue::Array(vec![PlistValue::Dictionary(streams)]),
            );
            d.insert(
                "timingProtocol".to_string(),
                PlistValue::String("PTP".into()),
            );
            d
        }))
        .map_err(|e| MockSenderError::Protocol(e.to_string()))?;

        let request = self.build_request(Method::Setup, "/setup", Some(body));
        let _response = self.send_request(&request).await?;

        Ok((7011, 7010)) // Placeholder ports
    }

    /// SETUP phase 2 (audio)
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn setup_audio(&mut self) -> Result<(u16, u16), MockSenderError> {
        let mut streams = HashMap::new();
        streams.insert("type".to_string(), PlistValue::UnsignedInteger(96)); // Audio
        streams.insert("ct".to_string(), PlistValue::UnsignedInteger(100)); // PCM
        streams.insert("sr".to_string(), PlistValue::UnsignedInteger(44100));
        streams.insert("ch".to_string(), PlistValue::UnsignedInteger(2));
        streams.insert("ss".to_string(), PlistValue::UnsignedInteger(16));

        let body = encode_bplist_body(&PlistValue::Dictionary({
            let mut d = HashMap::new();
            d.insert(
                "streams".to_string(),
                PlistValue::Array(vec![PlistValue::Dictionary(streams)]),
            );
            d
        }))
        .map_err(|e| MockSenderError::Protocol(e.to_string()))?;

        let request = self.build_request(Method::Setup, "/setup", Some(body));
        let _response = self.send_request(&request).await?;

        Ok((7100, 7101)) // Placeholder ports
    }

    /// Send RECORD
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn record(&mut self) -> Result<(), MockSenderError> {
        let request = self.build_request(Method::Record, "/record", None);
        let _response = self.send_request(&request).await?;
        Ok(())
    }

    /// Send audio packet
    ///
    /// # Errors
    /// Returns `MockSenderError` if send fails.
    pub fn send_audio(&self, _samples: &[i16], _timestamp: u32) -> Result<(), MockSenderError> {
        // Would send encrypted RTP packet
        Ok(())
    }

    /// Send TEARDOWN
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn teardown(&mut self) -> Result<(), MockSenderError> {
        let request = self.build_request(Method::Teardown, "/teardown", None);
        let _response = self.send_request(&request).await?;
        Ok(())
    }

    /// Build an RTSP request.
    pub fn build_request(
        &mut self,
        method: Method,
        uri: &str,
        body: Option<Vec<u8>>,
    ) -> RtspRequest {
        self.cseq += 1;

        let mut headers = Headers::new();
        headers.insert("CSeq".to_string(), self.cseq.to_string());
        headers.insert("User-Agent".to_string(), "MockSender/1.0".to_string());

        if let Some(ref session) = self.session_id {
            headers.insert("Session".to_string(), session.clone());
        }

        if let Some(ref b) = body {
            headers.insert("Content-Length".to_string(), b.len().to_string());
            headers.insert(
                "Content-Type".to_string(),
                "application/x-apple-binary-plist".to_string(),
            );
        }

        RtspRequest {
            method,
            uri: uri.to_string(),
            headers,
            body: body.unwrap_or_default(),
        }
    }

    /// Send an RTSP request and return the response bytes.
    ///
    /// # Errors
    /// Returns `MockSenderError` if sending fails.
    pub async fn send_request(
        &mut self,
        request: &RtspRequest,
    ) -> Result<Vec<u8>, MockSenderError> {
        let stream = self.stream.as_mut().ok_or(MockSenderError::NotConnected)?;

        // Serialize and send request
        let request_bytes = Self::serialize_request(request);

        // Optionally encrypt if key is set
        let to_send = if let Some(_key) = self.encryption_key {
            // Encrypt with HAP framing
            request_bytes // Simplified
        } else {
            request_bytes
        };

        stream.write_all(&to_send).await?;

        // Read response
        let mut response = vec![0u8; 4096];
        let n = stream.read(&mut response).await?;
        response.truncate(n);

        Ok(response)
    }

    fn serialize_request(request: &RtspRequest) -> Vec<u8> {
        let mut output = Vec::new();

        // Request line
        output.extend_from_slice(
            format!("{} {} RTSP/1.0\r\n", request.method.as_str(), request.uri).as_bytes(),
        );

        // Headers
        for (name, value) in request.headers.iter() {
            output.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        output.extend_from_slice(b"\r\n");

        // Body
        output.extend_from_slice(&request.body);

        output
    }
}

/// Result of a mock session
#[derive(Debug)]
pub struct MockSessionResult {
    /// Timing port
    pub timing_port: u16,
    /// Audio data port
    pub audio_data_port: u16,
    /// Audio control port
    pub audio_control_port: u16,
}

/// Errors from the mock sender
#[derive(Debug, thiserror::Error)]
pub enum MockSenderError {
    /// Not connected to receiver
    #[error("Not connected")]
    NotConnected,

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Protocol error
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Pairing failed
    #[error("Pairing failed: {0}")]
    PairingFailed(String),
}
