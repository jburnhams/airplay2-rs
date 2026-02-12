//! Connection manager for `AirPlay` devices
#![allow(dead_code)]

use super::state::{ConnectionEvent, ConnectionState, ConnectionStats, DisconnectReason};
use crate::audio::AudioCodec;
use crate::error::AirPlayError;
use crate::net::{AsyncReadExt, AsyncWriteExt, Runtime, TcpStream};
use crate::protocol::pairing::{
    AuthSetup, PairSetup, PairVerify, PairingKeys, PairingStepResult, PairingStorage, SessionKeys,
    TransientPairing,
};
use crate::protocol::rtsp::{Method, RtspCodec, RtspRequest, RtspResponse, RtspSession};
use crate::types::{AirPlayConfig, AirPlayDevice};

use std::fmt::Write;
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock, broadcast};

/// Connection manager handles device connections
pub struct ConnectionManager {
    /// Configuration
    config: AirPlayConfig,
    /// Current state
    state: RwLock<ConnectionState>,
    /// Connected device info
    device: RwLock<Option<AirPlayDevice>>,
    /// TCP connection
    stream: Mutex<Option<TcpStream>>,
    /// UDP sockets (audio, control, timing)
    sockets: Mutex<Option<UdpSockets>>,
    /// RTSP session
    rtsp_session: Mutex<Option<RtspSession>>,
    /// RTSP codec
    rtsp_codec: Mutex<RtspCodec>,
    /// Session keys (after pairing)
    session_keys: Mutex<Option<SessionKeys>>,
    /// Secure session (HAP encryption)
    secure_session: Mutex<Option<crate::net::secure::HapSecureSession>>,
    /// Buffer for decrypted data
    decrypted_buffer: Mutex<Vec<u8>>,
    /// Connection statistics
    stats: RwLock<ConnectionStats>,
    /// Event sender
    event_tx: broadcast::Sender<ConnectionEvent>,
    /// Pairing storage
    pairing_storage: Mutex<Option<Box<dyn PairingStorage>>>,
}

/// UDP sockets for streaming
#[allow(dead_code)]
struct UdpSockets {
    audio: UdpSocket,
    control: UdpSocket,
    timing: UdpSocket,
    #[allow(dead_code)]
    server_audio_port: u16,
    #[allow(dead_code)]
    server_control_port: u16,
    #[allow(dead_code)]
    server_timing_port: u16,
}

impl ConnectionManager {
    /// Create a new connection manager
    #[must_use]
    pub fn new(config: AirPlayConfig) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        Self {
            config,
            state: RwLock::new(ConnectionState::Disconnected),
            device: RwLock::new(None),
            stream: Mutex::new(None),
            sockets: Mutex::new(None),
            rtsp_session: Mutex::new(None),
            rtsp_codec: Mutex::new(RtspCodec::new()),
            session_keys: Mutex::new(None),
            secure_session: Mutex::new(None),
            decrypted_buffer: Mutex::new(Vec::new()),
            stats: RwLock::new(ConnectionStats::default()),
            event_tx,
            pairing_storage: Mutex::new(None),
        }
    }

    /// Set pairing storage for persistent pairing
    #[must_use]
    pub fn with_pairing_storage(mut self, storage: Box<dyn PairingStorage>) -> Self {
        self.pairing_storage = Mutex::new(Some(storage));
        self
    }

    /// Get current connection state
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Get connected device
    pub async fn device(&self) -> Option<AirPlayDevice> {
        self.device.read().await.clone()
    }

    /// Get connection statistics
    pub async fn stats(&self) -> ConnectionStats {
        self.stats.read().await.clone()
    }

    /// Get the session encryption key for audio (raw shared secret)
    pub async fn encryption_key(&self) -> Option<[u8; 32]> {
        self.session_keys
            .lock()
            .await
            .as_ref()
            .map(|k| k.raw_shared_secret)
    }

    /// Connect to a device
    ///
    /// # Errors
    ///
    /// Returns error if connection or pairing fails
    pub async fn connect(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        // Check if already connected
        let current_state = *self.state.read().await;
        if current_state.is_active() {
            return Err(AirPlayError::InvalidState {
                message: "Already connected or connecting".to_string(),
                current_state: format!("{current_state:?}"),
            });
        }

        self.set_state(ConnectionState::Connecting).await;
        *self.device.write().await = Some(device.clone());

        // Attempt connection with timeout
        let result = Runtime::timeout(
            self.config.connection_timeout,
            self.connect_internal(device),
        )
        .await;

        match result {
            Ok(Ok(())) => {
                self.set_state(ConnectionState::Connected).await;
                self.send_event(ConnectionEvent::Connected {
                    device: device.clone(),
                });
                Ok(())
            }
            Ok(Err(e)) => {
                self.set_state(ConnectionState::Failed).await;
                self.send_event(ConnectionEvent::Error {
                    message: e.to_string(),
                    recoverable: e.is_recoverable(),
                });
                Err(e)
            }
            Err(_) => {
                self.set_state(ConnectionState::Failed).await;
                Err(AirPlayError::ConnectionTimeout {
                    duration: self.config.connection_timeout,
                })
            }
        }
    }

    /// Internal connection logic
    async fn connect_internal(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        // 1. Establish TCP connection
        let addr = format!("{}:{}", device.address(), device.port);
        tracing::debug!("Connecting to {}", addr);

        let stream =
            TcpStream::connect(&addr)
                .await
                .map_err(|e| AirPlayError::ConnectionFailed {
                    device_name: device.name.clone(),
                    message: e.to_string(),
                    source: Some(Box::new(e)),
                })?;

        *self.stream.lock().await = Some(stream);

        // 2. Initialize RTSP session
        let rtsp_session = RtspSession::new(&device.address().to_string(), device.port);
        *self.rtsp_session.lock().await = Some(rtsp_session);

        // 3. Perform OPTIONS exchange
        self.set_state(ConnectionState::SettingUp).await;
        self.send_options().await?;

        // 3.5. Try GET /info to check connectivity/auth state
        tracing::debug!("Sending GET /info...");
        let mut manufacturer = String::new();
        match self.send_get_command("/info").await {
            Ok(body) => {
                if let Ok(plist) = crate::protocol::plist::decode(&body) {
                    tracing::debug!("GET /info success. Parsed plist: {:#?}", plist);
                    if let Some(m) = plist
                        .as_dict()
                        .and_then(|d| d.get("manufacturer"))
                        .and_then(|v| v.as_str())
                    {
                        manufacturer = m.to_string();
                    }
                } else {
                    tracing::debug!("GET /info success (binary): {} bytes", body.len());
                }
            }
            Err(e) => tracing::warn!("GET /info failed: {}", e),
        }

        // 4. Authenticate if required
        self.set_state(ConnectionState::Authenticating).await;

        // 4.1 Perform Auth-Setup (MFi handshake)
        // Some devices (like Sonos) fail 403 on pair-setup if this is not done first.
        // We skip it for OpenAirplay (python) as it expects FairPlay plist.
        if manufacturer == "OpenAirplay" {
            tracing::info!("Skipping Auth-Setup for OpenAirplay device");
        } else {
            match self.auth_setup().await {
                Ok(()) => tracing::info!("Auth-Setup succeeded"),
                Err(e) => {
                    tracing::warn!(
                        "Auth-Setup failed (might be optional for some devices): {}",
                        e
                    );
                }
            }
        }

        self.authenticate(device).await?;

        // 5. Setup RTSP session
        self.set_state(ConnectionState::SettingUp).await;

        self.setup_session().await?;

        Ok(())
    }

    /// Send RTSP OPTIONS and process response
    async fn send_options(&self) -> Result<(), AirPlayError> {
        let request = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session.options_request()
        };

        let response = self.send_rtsp_request(&request).await?;

        {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "RTSP session closed during OPTIONS request".to_string(),
                    current_state: "Disconnected".to_string(),
                })?;

            session
                .process_response(Method::Options, &response)
                .map_err(|e| AirPlayError::RtspError {
                    message: e,
                    status_code: Some(response.status.as_u16()),
                })?;
        }

        Ok(())
    }

    /// Perform Auth-Setup handshake
    async fn auth_setup(&self) -> Result<(), AirPlayError> {
        let auth = AuthSetup::new();
        let body = auth.start();

        tracing::debug!("Sending POST /auth-setup...");
        let response = self
            .send_post_command(
                "/auth-setup",
                Some(body),
                Some("application/octet-stream".to_string()),
            )
            .await
            .map_err(|e| {
                // Some devices might not support/require auth-setup, or return 404 if not needed
                // But usually AirPlay 2 devices do.
                tracing::warn!("Auth-Setup failed: {}", e);
                e
            })?;

        tracing::debug!("Received Auth-Setup response: {} bytes", response.len());

        auth.process_response(&response)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: format!("Auth-Setup response invalid: {e}"),
                recoverable: false,
            })?;

        tracing::info!("Auth-Setup completed successfully.");
        Ok(())
    }

    /// Authenticate with the device
    async fn authenticate(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        // Check if we have stored keys
        if let Some(ref storage) = *self.pairing_storage.lock().await {
            if let Some(keys) = storage.load(&device.id).await {
                // Try Pair-Verify with stored keys
                match self.pair_verify(device, &keys).await {
                    Ok(session_keys) => {
                        *self.session_keys.lock().await = Some(session_keys);
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::warn!("Pair-Verify failed, trying transient: {}", e);
                    }
                }
            }
        }

        // Try configured PIN first if available
        if let Some(ref pin) = self.config.pin {
            tracing::info!("Attempting SRP Pairing with configured PIN: '{}'...", pin);

            // Try standard usernames with the configured PIN
            let usernames = ["Pair-Setup", "AirPlay", "admin"];

            for user in usernames {
                match self.pair_setup(user, pin).await {
                    Ok((session_keys, pairing_keys)) => {
                        tracing::info!(
                            "SRP Pairing successful with configured PIN and User='{}'",
                            user
                        );
                        *self.secure_session.lock().await =
                            Some(crate::net::secure::HapSecureSession::new(
                                &session_keys.encrypt_key,
                                &session_keys.decrypt_key,
                            ));
                        *self.session_keys.lock().await = Some(session_keys);

                        // Save pairing keys if we have storage and keys were generated
                        if let (Some(ref mut storage), Some(keys)) =
                            (self.pairing_storage.lock().await.as_mut(), pairing_keys)
                        {
                            tracing::info!("Saving pairing keys for device {}", device.id);
                            if let Err(e) = storage.save(&device.id, &keys).await {
                                tracing::warn!("Failed to save pairing keys: {}", e);
                            }
                        }

                        return Ok(());
                    }
                    Err(e) => {
                        tracing::warn!(
                            "SRP Pairing failed with configured PIN and User='{}': {}",
                            user,
                            e
                        );
                        // If the error suggests connection loss, we might need to reconnect,
                        // but currently we just try the next username on the same stream.
                        // Ideally, we should check if stream is alive.
                    }
                }
            }

            // If configured PIN was provided but failed, we stop here.
            return Err(AirPlayError::AuthenticationFailed {
                message: "Authentication failed with configured PIN".to_string(),
                recoverable: false,
            });
        }

        // Try various credentials for SRP Pairing
        // Format: (username, pin)
        let credentials = [
            ("Pair-Setup", "3939"), // Standard AirPort
            ("Pair-Setup", "0000"),
            ("Pair-Setup", "1111"),
            ("Pair-Setup", "1234"),
            ("3939", "3939"),
            ("admin", "3939"),
            ("AirPlay", "3939"),
            ("Pair-Setup", ""), // Empty PIN
        ];

        for (user, pin) in credentials {
            tracing::info!("Attempting SRP Pairing: User='{}', PIN='{}'...", user, pin);
            match self.pair_setup(user, pin).await {
                Ok((session_keys, pairing_keys)) => {
                    tracing::info!("SRP Pairing successful with User='{}', PIN='{}'", user, pin);
                    *self.secure_session.lock().await =
                        Some(crate::net::secure::HapSecureSession::new(
                            &session_keys.encrypt_key,
                            &session_keys.decrypt_key,
                        ));
                    *self.session_keys.lock().await = Some(session_keys);

                    // Save pairing keys if we have storage and keys were generated
                    if let (Some(ref mut storage), Some(keys)) =
                        (self.pairing_storage.lock().await.as_mut(), pairing_keys)
                    {
                        tracing::info!("Saving pairing keys for device {}", device.id);
                        if let Err(e) = storage.save(&device.id, &keys).await {
                            tracing::warn!("Failed to save pairing keys: {}", e);
                        }
                    }

                    return Ok(());
                }
                Err(e) => {
                    tracing::debug!("SRP Pairing failed: {}", e);
                    // Wait a bit to avoid backoff
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }

        // 3. Fail if neither worked
        Err(AirPlayError::AuthenticationFailed {
            message: "All pairing methods failed".to_string(),
            recoverable: false,
        })
    }

    /// Perform Pair-Setup with PIN (SRP)
    async fn pair_setup(
        &self,
        username: &str,
        pin: &str,
    ) -> Result<(SessionKeys, Option<PairingKeys>), AirPlayError> {
        let mut pairing = PairSetup::new();
        pairing.set_username(username);
        pairing.set_pin(pin);

        // If PIN is "3939", assume transient mode (for AirPort Express 2)
        // Note: For persistent pairing test, we disable this override.
        // In a real app, this logic needs to be smarter (maybe try both?)
        /*if pin == "3939" {
            pairing.set_transient(true);
        }*/

        // M1: Start pairing
        let m1 = pairing
            .start()
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        tracing::debug!("Starting Pair-Setup (SRP)...");
        let m2 = self.send_pairing_data(&m1, "/pair-setup").await?;

        // M2 -> M3
        let result = pairing
            .process_m2(&m2)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        let PairingStepResult::SendData(m3) = result else {
            return Err(AirPlayError::AuthenticationFailed {
                message: "Unexpected pairing state after M2".to_string(),
                recoverable: false,
            });
        };

        tracing::debug!("Sending M3...");
        let m4 = self.send_pairing_data(&m3, "/pair-setup").await?;

        // M4 -> M5 (or Complete if transient)
        let result = pairing
            .process_m4(&m4)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        if let PairingStepResult::Complete(keys) = result {
            tracing::info!("Pairing completed early (Transient Mode)");
            return Ok((keys, None));
        }

        let PairingStepResult::SendData(m5) = result else {
            return Err(AirPlayError::AuthenticationFailed {
                message: "Unexpected pairing state after M4".to_string(),
                recoverable: false,
            });
        };

        tracing::debug!("Sending M5...");
        let m6 = self.send_pairing_data(&m5, "/pair-setup").await?;

        // M6 -> Complete
        let result = pairing
            .process_m6(&m6)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        match result {
            PairingStepResult::Complete(keys) => {
                // Construct pairing keys if we have device public key
                let pairing_keys = if let Some(device_pk) = pairing.device_public_key() {
                    let mut device_public_key = [0u8; 32];
                    if device_pk.len() == 32 {
                        device_public_key.copy_from_slice(device_pk);
                        Some(PairingKeys {
                            identifier: b"airplay2-rs".to_vec(),
                            secret_key: pairing.our_secret_key(),
                            public_key: pairing.our_public_key(),
                            device_public_key,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok((keys, pairing_keys))
            }
            _ => Err(AirPlayError::AuthenticationFailed {
                message: "Pairing did not complete".to_string(),
                recoverable: false,
            }),
        }
    }

    /// Perform transient pairing
    async fn transient_pair(&self) -> Result<SessionKeys, AirPlayError> {
        let mut pairing = TransientPairing::new();

        // M1: Start pairing
        let m1 = pairing
            .start()
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        tracing::debug!("Starting Transient Pairing (M1)...");
        let m2 = self.send_pairing_data(&m1, "/pair-setup").await?;
        tracing::debug!("Received M2 ({} bytes)", m2.len());

        // M2 -> M3
        let result = pairing
            .process_m2(&m2)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        if let PairingStepResult::Complete(keys) = result {
            return Ok(keys);
        }

        let PairingStepResult::SendData(m3) = result else {
            return Err(AirPlayError::AuthenticationFailed {
                message: "Unexpected pairing state".to_string(),
                recoverable: false,
            });
        };

        tracing::debug!("Sending M3...");
        let m4 = self.send_pairing_data(&m3, "/pair-setup").await?;
        tracing::debug!("Received M4 ({} bytes)", m4.len());

        // M4 -> Complete
        let result = pairing
            .process_m4(&m4)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        match result {
            PairingStepResult::Complete(keys) => {
                tracing::info!("Transient Pairing completed successfully.");
                Ok(keys)
            }
            _ => Err(AirPlayError::AuthenticationFailed {
                message: "Pairing did not complete".to_string(),
                recoverable: false,
            }),
        }
    }

    /// Perform Pair-Verify with stored keys
    async fn pair_verify(
        &self,
        _device: &AirPlayDevice,
        keys: &PairingKeys,
    ) -> Result<SessionKeys, AirPlayError> {
        let mut pairing = PairVerify::new(keys.clone(), &keys.device_public_key).map_err(|e| {
            AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            }
        })?;

        // M1: Start verification
        let m1 = pairing
            .start()
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        let m2 = self.send_pairing_data(&m1, "/pair-verify").await?;

        // M2 -> M3
        let result = pairing
            .process_m2(&m2)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        let PairingStepResult::SendData(m3) = result else {
            return Err(AirPlayError::AuthenticationFailed {
                message: "Unexpected pairing state".to_string(),
                recoverable: false,
            });
        };

        let m4 = self.send_pairing_data(&m3, "/pair-verify").await?;

        // M4 -> Complete
        let result = pairing
            .process_m4(&m4)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        match result {
            PairingStepResult::Complete(keys) => Ok(keys),
            _ => Err(AirPlayError::AuthenticationFailed {
                message: "Verification did not complete".to_string(),
                recoverable: false,
            }),
        }
    }

    /// Setup RTSP session (`AirPlay` 2 sequence)
    #[allow(clippy::too_many_lines)]
    async fn setup_session(&self) -> Result<(), AirPlayError> {
        use crate::protocol::plist::DictBuilder;

        // 1. GET /info (Encrypted) - Some devices refresh state here
        tracing::debug!("Performing GET /info (Encrypted)...");
        let _ = self.send_get_command("/info").await?;

        // 2. Session Setup (SETUP / with Plist)
        tracing::debug!("Performing Session SETUP...");
        let group_uuid = "D67B1696-8D3A-A6CF-9ACF-03C837DC68FD";
        let setup_plist = DictBuilder::new()
            .insert("timingProtocol", "NTP")
            .insert("groupUUID", group_uuid)
            .insert("macAddress", "AC:07:75:12:4A:1F")
            .insert("isAudioReceiver", false)
            .build();

        let setup_session_req = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session.setup_session_request(&setup_plist)
        };
        self.send_rtsp_request(&setup_session_req).await?;

        // 3. Announce (ANNOUNCE / with SDP)
        tracing::debug!("Performing ANNOUNCE...");
        // Note: We omit rsaaeskey/aesiv to force usage of session key (ChaCha20-Poly1305)
        // Build SDP based on configured codec
        let sdp = match self.config.audio_codec {
            AudioCodec::Alac => {
                // ALAC negotiation (96 AppleLossless)
                // Note: Python receiver expects exactly 'AppleLossless' (no /44100/2)
                "v=0\r\no=- 0 0 IN IP4 0.0.0.0\r\ns=airplay2-rs\r\nc=IN IP4 0.0.0.0\r\nt=0 0\r\nm=audio 0 RTP/AVP 96\r\na=rtpmap:96 AppleLossless\r\na=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n".to_string()
            }
            AudioCodec::Pcm => {
                // PCM/L16 negotiation (uncompressed audio)
                "v=0\r\no=- 0 0 IN IP4 0.0.0.0\r\ns=airplay2-rs\r\nc=IN IP4 0.0.0.0\r\nt=0 0\r\nm=audio 0 RTP/AVP 96\r\na=rtpmap:96 L16/44100/2\r\na=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n".to_string()
            }
            AudioCodec::Aac => {
                // AAC negotiation (96 mpeg4-generic)
                // mode=AAC-hbr implies RFC 3640 (requires AU headers)
                "v=0\r\no=- 0 0 IN IP4 0.0.0.0\r\ns=airplay2-rs\r\nc=IN IP4 0.0.0.0\r\nt=0 0\r\nm=audio 0 RTP/AVP 96\r\na=rtpmap:96 mpeg4-generic/44100/2\r\na=fmtp:96 mode=AAC-hbr;sizelength=13;indexlength=3;indexdeltalength=3;constantDuration=1024\r\n".to_string()
            }
            AudioCodec::Opus => {
                return Err(AirPlayError::InvalidParameter {
                    name: "audio_codec".to_string(),
                    message: "Opus codec not yet supported for SDP generation".to_string(),
                });
            }
        };

        let announce_req = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session.announce_request(&sdp)
        };
        self.send_rtsp_request(&announce_req).await?;

        // 3. Bind local UDP ports
        let audio_sock = UdpSocket::bind("0.0.0.0:0").await?;
        let ctrl_sock = UdpSocket::bind("0.0.0.0:0").await?;
        let time_sock = UdpSocket::bind("0.0.0.0:0").await?;

        let _audio_port = audio_sock.local_addr()?.port();
        let ctrl_port = ctrl_sock.local_addr()?.port();
        let time_port = time_sock.local_addr()?.port();

        // 4. Stream Setup (SETUP /rtp/audio with Transport)
        tracing::debug!("Performing Stream SETUP...");
        let transport = format!(
            "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record;control_port={ctrl_port};timing_port={time_port}"
        );

        let setup_stream_req = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session.setup_stream_request(&transport)
        };

        let response = self.send_rtsp_request(&setup_stream_req).await?;

        // 5. Update session state
        {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session
                .process_response(Method::Setup, &response)
                .map_err(|e| AirPlayError::RtspError {
                    message: e,
                    status_code: Some(response.status.as_u16()),
                })?;
        }

        // 6. Parse response transport header to get server ports
        let transport_header =
            response
                .headers
                .get("Transport")
                .ok_or(AirPlayError::RtspError {
                    message: "Missing Transport header in SETUP response".to_string(),
                    status_code: None,
                })?;

        let (server_audio_port, server_ctrl_port, server_time_port) =
            Self::parse_transport_ports(transport_header)?;

        // 7. Connect UDP sockets to server ports
        let device_ip = {
            let current_state = self.state().await;
            let device_guard = self.device.read().await;
            let device = device_guard
                .as_ref()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "Device information is missing.".to_string(),
                    current_state: format!("{current_state:?}"),
                })?;
            device.address()
        };

        audio_sock.connect((device_ip, server_audio_port)).await?;
        ctrl_sock.connect((device_ip, server_ctrl_port)).await?;
        time_sock.connect((device_ip, server_time_port)).await?;

        *self.sockets.lock().await = Some(UdpSockets {
            audio: audio_sock,
            control: ctrl_sock,
            timing: time_sock,
            server_audio_port,
            server_control_port: server_ctrl_port,
            server_timing_port: server_time_port,
        });

        // 8. Send RECORD to start buffering
        tracing::debug!("Performing RECORD...");
        let record_request = {
            let mut session_guard = self.rtsp_session.lock().await;
            session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "Setup".to_string(),
                })?
                .record_request()
        };
        self.send_rtsp_request(&record_request).await?;

        Ok(())
    }

    /// Send pairing data to device
    async fn send_pairing_data(&self, data: &[u8], path: &str) -> Result<Vec<u8>, AirPlayError> {
        // Send as HTTP POST
        // Note: We need to include the standard RTSP/AirPlay headers here too,
        // as some devices reject bare HTTP POSTs without the correct User-Agent/identifiers.

        let (device_id, session_id, user_agent) = {
            let session_guard = self.rtsp_session.lock().await;
            if let Some(session) = session_guard.as_ref() {
                (
                    session.device_id().to_string(),
                    session.client_session_id().to_string(),
                    session.user_agent().to_string(),
                )
            } else {
                (String::new(), String::new(), "AirPlay/540.31".to_string())
            }
        };

        // Get device address for Host header (required for HTTP/1.1)
        let host = {
            let device_guard = self.device.read().await;
            if let Some(device) = device_guard.as_ref() {
                format!("{}:{}", device.address(), device.port)
            } else {
                "127.0.0.1:7000".to_string()
            }
        };

        // Construct request with all headers
        let mut request = format!(
            "POST {path} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Content-Type: application/octet-stream\r\n\
             Content-Length: {}\r\n\
             User-Agent: {user_agent}\r\n\
             Active-Remote: 4294967295\r\n\
             X-Apple-Client-Name: airplay2-rs\r\n",
            data.len()
        );

        if !device_id.is_empty() {
            let _ = write!(request, "DACP-ID: {device_id}\r\n");
            let _ = write!(request, "X-Apple-Device-ID: {device_id}\r\n");
        }

        if !session_id.is_empty() {
            let _ = write!(request, "X-Apple-Session-ID: {session_id}\r\n");
        }

        // Add X-Apple-HKP header for pairing requests
        // 3 = Normal, 4 = Transient
        // We default to 4 (Transient) as we are mostly trying 3939 flow
        if path.starts_with("/pair-setup") || path.starts_with("/pair-verify") {
            request.push_str("X-Apple-HKP: 4\r\n");
        }

        request.push_str("\r\n");

        let mut stream_guard = self.stream.lock().await;

        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| AirPlayError::Disconnected {
                device_name: "unknown".to_string(),
            })?;

        // Send request
        stream.write_all(request.as_bytes()).await?;
        stream.write_all(data).await?;
        stream.flush().await?;

        // Read headers
        let mut buf = Vec::new();
        let mut temp_buf = [0u8; 1];
        let mut body_start = 0;

        // Read byte by byte until double CRLF (inefficient but simple for now)
        // In a real implementation, use a buffered reader or proper parser
        while body_start == 0 {
            let n = stream.read(&mut temp_buf).await?;
            if n == 0 {
                return Err(AirPlayError::RtspError {
                    message: "Connection closed while reading headers".to_string(),
                    status_code: None,
                });
            }
            buf.push(temp_buf[0]);

            if buf.len() >= 4 && buf.ends_with(b"\r\n\r\n") {
                body_start = buf.len();
            }

            if buf.len() > 4096 {
                return Err(AirPlayError::RtspError {
                    message: "Headers too large".to_string(),
                    status_code: None,
                });
            }
        }

        // Parse Content-Length
        let headers_str =
            std::str::from_utf8(&buf[..body_start]).map_err(|_| AirPlayError::RtspError {
                message: "Invalid UTF-8 in headers".to_string(),
                status_code: None,
            })?;

        tracing::debug!("<< Pairing Response Headers:\n{}", headers_str.trim());

        let mut content_length = 0;
        for line in headers_str.lines() {
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = rest.trim().parse::<usize>().unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("content-length:") {
                content_length = rest.trim().parse::<usize>().unwrap_or(0);
            }
        }

        // Read body
        let mut body = vec![0u8; content_length];
        stream.read_exact(&mut body).await?;

        // Log pairing response body
        tracing::debug!(
            "<< Received Pairing Data ({} bytes): {:02X?}",
            body.len(),
            body
        );

        Ok(body)
    }

    /// Send RTSP request and get response
    async fn send_rtsp_request(&self, request: &RtspRequest) -> Result<RtspResponse, AirPlayError> {
        let encoded = request.encode();

        let mut secure_guard = self.secure_session.lock().await;
        let mut stream_guard = self.stream.lock().await;
        let stream = stream_guard
            .as_mut()
            .ok_or_else(|| AirPlayError::Disconnected {
                device_name: "unknown".to_string(),
            })?;

        if let Some(ref mut secure) = *secure_guard {
            tracing::debug!(
                ">> Sending Encrypted RTSP request ({} bytes)",
                encoded.len()
            );
            let encrypted = secure.encrypt(&encoded)?;
            stream.write_all(&encrypted).await?;
        } else {
            // Log outgoing request
            if let Ok(s) = std::str::from_utf8(&encoded) {
                tracing::debug!(">> Sending RTSP request:\n{}", s.trim());
            } else {
                tracing::debug!(">> Sending RTSP request (binary): {} bytes", encoded.len());
            }
            stream.write_all(&encoded).await?;
        }
        stream.flush().await?;

        // Update stats
        self.stats.write().await.record_sent(encoded.len());

        // Read response
        let mut codec = self.rtsp_codec.lock().await;
        let mut buf = vec![0u8; 4096];
        let mut encrypted_buf = Vec::new();

        loop {
            if let Some(response) = codec.decode().map_err(|e| AirPlayError::RtspError {
                message: e.to_string(),
                status_code: None,
            })? {
                return Ok(response);
            }

            let n = stream.read(&mut buf).await?;
            if n == 0 {
                return Err(AirPlayError::Disconnected {
                    device_name: "unknown".to_string(),
                });
            }

            if let Some(ref mut secure) = *secure_guard {
                use byteorder::{ByteOrder, LittleEndian};
                encrypted_buf.extend_from_slice(&buf[..n]);

                // Try to decrypt as many blocks as possible
                while encrypted_buf.len() >= 2 {
                    let block_len = LittleEndian::read_u16(&encrypted_buf[0..2]) as usize;
                    let total_len = 2 + block_len + 16;
                    if encrypted_buf.len() >= total_len {
                        let block = encrypted_buf.drain(..total_len).collect::<Vec<_>>();
                        let (decrypted, _) = secure.decrypt_block(&block)?;

                        if let Ok(s) = std::str::from_utf8(&decrypted) {
                            tracing::debug!("<< Received Decrypted RTSP data:\n{}", s.trim());
                        } else {
                            tracing::debug!(
                                "<< Received Decrypted RTSP data (binary): {} bytes",
                                decrypted.len()
                            );
                        }

                        codec
                            .feed(&decrypted)
                            .map_err(|e| AirPlayError::RtspError {
                                message: e.to_string(),
                                status_code: None,
                            })?;
                    } else {
                        break;
                    }
                }
            } else {
                // Log incoming data
                if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                    tracing::debug!("<< Received RTSP data:\n{}", s.trim());
                } else {
                    tracing::debug!("<< Received RTSP data (binary): {} bytes", n);
                }

                codec.feed(&buf[..n]).map_err(|e| AirPlayError::RtspError {
                    message: e.to_string(),
                    status_code: None,
                })?;
            }

            self.stats.write().await.record_received(n);
        }
    }

    /// Send RTP audio packet
    ///
    /// # Errors
    ///
    /// Returns error if sockets are not connected or send fails
    pub async fn send_rtp_audio(&self, packet: &[u8]) -> Result<(), AirPlayError> {
        let sockets = self.sockets.lock().await;
        if let Some(ref socks) = *sockets {
            socks
                .audio
                .send(packet)
                .await
                .map_err(|e| AirPlayError::RtspError {
                    message: format!("Failed to send RTP audio: {e}"),
                    status_code: None,
                })?;
            Ok(())
        } else {
            Err(AirPlayError::InvalidState {
                message: "RTP sockets not connected".to_string(),
                current_state: "Disconnected".to_string(),
            })
        }
    }

    /// Send an arbitrary RTSP command
    ///
    /// # Errors
    ///
    /// Returns error if command creation or sending fails
    pub async fn send_command(
        &self,
        method: Method,
        body: Option<Vec<u8>>,
        content_type: Option<String>,
    ) -> Result<Vec<u8>, AirPlayError> {
        let request = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;

            match method {
                Method::Play => {
                    let body = body.unwrap_or_default();
                    let content_type = content_type
                        .unwrap_or_else(|| "application/x-apple-binary-plist".to_string());
                    session.play_request(&content_type, body)
                }
                Method::SetParameter => {
                    let body = body.unwrap_or_default();
                    let content_type = content_type
                        .unwrap_or_else(|| "application/x-apple-binary-plist".to_string());
                    session.set_parameter_request(&content_type, body)
                }
                Method::GetParameter => {
                    session.get_parameter_request(content_type.as_deref(), body)
                }
                Method::Teardown => session.teardown_request(),
                Method::Pause => session.pause_request(),
                Method::SetRateAnchorTime => {
                    let body = body.unwrap_or_default();
                    let content_type = content_type
                        .unwrap_or_else(|| "application/x-apple-binary-plist".to_string());
                    session.set_rate_anchor_time_request(&content_type, body)
                }
                _ => {
                    return Err(AirPlayError::InvalidParameter {
                        name: "method".to_string(),
                        message: format!("Unsupported method for send_command: {method:?}"),
                    });
                }
            }
        };

        let response = self.send_rtsp_request(&request).await?;

        // Update session state
        {
            let mut session_guard = self.rtsp_session.lock().await;
            if let Some(session) = session_guard.as_mut() {
                session.process_response(method, &response).map_err(|e| {
                    AirPlayError::RtspError {
                        message: e,
                        status_code: Some(response.status.as_u16()),
                    }
                })?;
            }
        }

        Ok(response.body)
    }

    /// Send a POST request (for DACP or other controls)
    ///
    /// # Errors
    ///
    /// Returns error if command creation or sending fails
    pub async fn send_post_command(
        &self,
        path: &str,
        body: Option<Vec<u8>>,
        content_type: Option<String>,
    ) -> Result<Vec<u8>, AirPlayError> {
        let request = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;

            let body = body.unwrap_or_default();
            let content_type =
                content_type.unwrap_or_else(|| "application/x-apple-binary-plist".to_string());
            session.post_request(path, &content_type, body)
        };

        let response = self.send_rtsp_request(&request).await?;

        // Update session state
        {
            let mut session_guard = self.rtsp_session.lock().await;
            if let Some(session) = session_guard.as_mut() {
                session
                    .process_response(Method::Post, &response)
                    .map_err(|e| AirPlayError::RtspError {
                        message: e,
                        status_code: Some(response.status.as_u16()),
                    })?;
            }
        }

        Ok(response.body)
    }

    /// Send a GET request
    ///
    /// # Errors
    ///
    /// Returns error if command creation or sending fails
    pub async fn send_get_command(&self, path: &str) -> Result<Vec<u8>, AirPlayError> {
        let request = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session.get_request(path)
        };

        let response = self.send_rtsp_request(&request).await?;

        // Log response
        if let Ok(s) = std::str::from_utf8(&response.body) {
            tracing::debug!("GET {} response:\n{}", path, s);
        }

        Ok(response.body)
    }

    /// Disconnect from device
    ///
    /// # Errors
    ///
    /// Returns error if disconnection fails
    pub async fn disconnect(&self) -> Result<(), AirPlayError> {
        let device = self.device.read().await.clone();

        // Send TEARDOWN if connected
        if self.state().await == ConnectionState::Connected {
            let request = {
                let mut session = self.rtsp_session.lock().await;
                session.as_mut().map(RtspSession::teardown_request)
            };

            if let Some(request) = request {
                let _ = self.send_rtsp_request(&request).await;
            }
        }

        // Close connection
        *self.stream.lock().await = None;
        *self.sockets.lock().await = None;
        *self.rtsp_session.lock().await = None;
        *self.session_keys.lock().await = None;

        self.set_state(ConnectionState::Disconnected).await;

        if let Some(device) = device {
            self.send_event(ConnectionEvent::Disconnected {
                device,
                reason: DisconnectReason::UserRequested,
            });
        }

        Ok(())
    }

    /// Set connection state and emit event
    async fn set_state(&self, new_state: ConnectionState) {
        let old_state = {
            let mut state = self.state.write().await;
            let old = *state;
            *state = new_state;
            old
        };

        if old_state != new_state {
            self.send_event(ConnectionEvent::StateChanged {
                old: old_state,
                new: new_state,
            });
        }
    }

    /// Send an event
    fn send_event(&self, event: ConnectionEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Subscribe to connection events
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ConnectionEvent> {
        self.event_tx.subscribe()
    }

    fn parse_transport_ports(transport_header: &str) -> Result<(u16, u16, u16), AirPlayError> {
        let mut server_audio_port = 0;
        let mut server_ctrl_port = 0;
        let mut server_time_port = 0;

        for part in transport_header.split(';') {
            if let Some((key, value)) = part.trim().split_once('=') {
                if let Ok(port) = value.parse::<u16>() {
                    match key {
                        "server_port" => server_audio_port = port,
                        "control_port" => server_ctrl_port = port,
                        "timing_port" => server_time_port = port,
                        _ => {}
                    }
                }
            }
        }

        if server_audio_port == 0 {
            return Err(AirPlayError::RtspError {
                message: "Could not determine server audio port".to_string(),
                status_code: None,
            });
        }

        Ok((server_audio_port, server_ctrl_port, server_time_port))
    }
}
