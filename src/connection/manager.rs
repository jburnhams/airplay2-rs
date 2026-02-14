//! Connection manager for `AirPlay` devices
#![allow(dead_code)]

use super::state::{ConnectionEvent, ConnectionState, ConnectionStats, DisconnectReason};
use crate::audio::AudioCodec;
use crate::error::AirPlayError;
use crate::net::{AsyncReadExt, AsyncWriteExt, Runtime, TcpStream};
use crate::protocol::pairing::{
    AuthSetup, PairSetup, PairVerify, PairingKeys, PairingStepResult, PairingStorage, SessionKeys,
};
use crate::protocol::ptp::{PtpHandlerConfig, PtpRole, SharedPtpClock, create_shared_clock};
use crate::protocol::rtsp::{Method, RtspCodec, RtspRequest, RtspResponse, RtspSession};
use crate::types::{AirPlayConfig, AirPlayDevice, TimingProtocol};

use std::fmt::Write;
use std::sync::Arc;
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
    /// Shared PTP clock state (available after PTP timing is started)
    ptp_clock: Mutex<Option<SharedPtpClock>>,
    /// Shutdown signal sender for PTP handler task
    ptp_shutdown_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    /// Whether PTP timing is active for the current session
    ptp_active: RwLock<bool>,
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
            ptp_clock: Mutex::new(None),
            ptp_shutdown_tx: Mutex::new(None),
            ptp_active: RwLock::new(false),
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
        *self.secure_session.lock().await = None;
        *self.session_keys.lock().await = None;

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
        // 1. Try Transient Pairing first (most common for HomePods allowing it)
        if self.try_transient_pairing().await.is_ok() {
            return Ok(());
        }

        // 2. Check if we have stored keys
        if self.try_stored_keys(device).await.is_ok() {
            return Ok(());
        }

        // 3. Try configured PIN first if available
        if let Some(ref pin) = self.config.pin {
            return self.try_configured_pin(device, pin).await;
        }

        // 4. Try various credentials for SRP Pairing
        self.try_brute_force_pairing(device).await
    }

    async fn try_transient_pairing(&self) -> Result<(), ()> {
        tracing::info!("Attempting Transient Pairing...");
        match self.transient_pair().await {
            Ok(session_keys) => {
                tracing::info!("Transient Pairing successful");
                *self.secure_session.lock().await =
                    Some(crate::net::secure::HapSecureSession::new(
                        &session_keys.encrypt_key,
                        &session_keys.decrypt_key,
                    ));
                *self.session_keys.lock().await = Some(session_keys);
                Ok(())
            }
            Err(e) => {
                if let AirPlayError::AuthenticationFailed { message, .. } = &e {
                    tracing::debug!("Transient Pairing failed: {}", message);
                } else {
                    tracing::warn!("Transient Pairing failed: {}", e);
                }
                Err(())
            }
        }
    }

    async fn try_stored_keys(&self, device: &AirPlayDevice) -> Result<(), ()> {
        if let Some(ref storage) = *self.pairing_storage.lock().await {
            if let Some(keys) = storage.load(&device.id).await {
                match self.pair_verify(device, &keys).await {
                    Ok(session_keys) => {
                        *self.session_keys.lock().await = Some(session_keys);
                        return Ok(());
                    }
                    Err(e) => {
                        tracing::warn!("Pair-Verify failed, trying PIN: {}", e);
                    }
                }
            }
        }
        Err(())
    }

    async fn try_configured_pin(
        &self,
        device: &AirPlayDevice,
        pin: &str,
    ) -> Result<(), AirPlayError> {
        tracing::info!("Attempting SRP Pairing with configured PIN: '{}'...", pin);
        let usernames = ["Pair-Setup", "AirPlay", "admin"];

        for user in usernames {
            if let Ok((session_keys, pairing_keys)) = self.pair_setup(user, pin).await {
                self.handle_pairing_success(device, session_keys, pairing_keys)
                    .await;
                return Ok(());
            }
        }
        Err(AirPlayError::AuthenticationFailed {
            message: "Authentication failed with configured PIN".to_string(),
            recoverable: false,
        })
    }

    async fn try_brute_force_pairing(&self, device: &AirPlayDevice) -> Result<(), AirPlayError> {
        let credentials = [
            ("Pair-Setup", "3939"),
            ("Pair-Setup", "0000"),
            ("Pair-Setup", "1111"),
            ("Pair-Setup", "1234"),
            ("3939", "3939"),
            ("admin", "3939"),
            ("AirPlay", "3939"),
            ("Pair-Setup", ""),
        ];

        for (user, pin) in credentials {
            tracing::info!("Attempting SRP Pairing: User='{}', PIN='{}'...", user, pin);
            match self.pair_setup(user, pin).await {
                Ok((session_keys, pairing_keys)) => {
                    self.handle_pairing_success(device, session_keys, pairing_keys)
                        .await;
                    return Ok(());
                }
                Err(e) => {
                    tracing::debug!("SRP Pairing failed: {}", e);
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }

        Err(AirPlayError::AuthenticationFailed {
            message: "All pairing methods failed".to_string(),
            recoverable: false,
        })
    }

    async fn handle_pairing_success(
        &self,
        device: &AirPlayDevice,
        session_keys: SessionKeys,
        pairing_keys: Option<PairingKeys>,
    ) {
        tracing::info!("SRP Pairing successful");
        *self.secure_session.lock().await = Some(crate::net::secure::HapSecureSession::new(
            &session_keys.encrypt_key,
            &session_keys.decrypt_key,
        ));
        *self.session_keys.lock().await = Some(session_keys);

        if let (Some(ref mut storage), Some(keys)) =
            (self.pairing_storage.lock().await.as_mut(), pairing_keys)
        {
            let _ = storage.save(&device.id, &keys).await;
        }
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

    /// Perform transient pairing using SRP (Pair-Setup with transient flag)
    async fn transient_pair(&self) -> Result<SessionKeys, AirPlayError> {
        let mut pairing = PairSetup::new();
        pairing.set_transient(true);
        pairing.set_pin("3939");
        pairing.set_username("Pair-Setup");

        // M1: Start pairing
        let m1 = pairing
            .start()
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        tracing::debug!("Starting Transient Pairing (SRP+Transient)...");
        let m2 = self.send_pairing_data(&m1, "/pair-setup").await?;
        tracing::debug!("Received M2 ({} bytes)", m2.len());

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
        tracing::debug!("Received M4 ({} bytes)", m4.len());

        // M4 -> Complete (since transient=true)
        let result = pairing
            .process_m4(&m4)
            .map_err(|e| AirPlayError::AuthenticationFailed {
                message: e.to_string(),
                recoverable: false,
            })?;

        match result {
            PairingStepResult::Complete(keys) => {
                tracing::info!("Transient Pairing completed (SRP M4)");
                Ok(keys)
            }
            PairingStepResult::SendData(_) => Err(AirPlayError::AuthenticationFailed {
                message: "Unexpected continuation after M4 in transient mode".to_string(),
                recoverable: false,
            }),
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

        // 2. Session Setup (SETUP / with Plist) — only for NTP/AirPlay 1 devices
        let group_uuid = "D67B1696-8D3A-A6CF-9ACF-03C837DC68FD";

        // Determine timing protocol based on config and device capabilities
        let use_ptp = self.should_use_ptp().await;
        let timing_protocol_str = if use_ptp { "PTP" } else { "NTP" };
        tracing::info!("Using timing protocol: {}", timing_protocol_str);

        if !use_ptp {
            // For NTP/AirPlay 1 devices, send a preliminary Session SETUP
            tracing::debug!("Performing Session SETUP (NTP)...");
            let setup_plist = DictBuilder::new()
                .insert("timingProtocol", timing_protocol_str)
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
                session.setup_session_request(&setup_plist, None)
            };
            self.send_rtsp_request(&setup_session_req).await?;
        }

        // 3. Announce (ANNOUNCE / with SDP) — skip for PTP/Buffered Audio devices
        // AirPlay 2 Buffered Audio negotiates format via SETUP plist, not ANNOUNCE SDP.
        // Sending ANNOUNCE to HomePod returns 455 and may corrupt session state.
        if use_ptp {
            tracing::info!("Skipping ANNOUNCE for PTP/Buffered Audio device");
        } else {
            tracing::debug!("Performing ANNOUNCE...");
            let sdp = match self.config.audio_codec {
                AudioCodec::Alac => {
                    "v=0\r\no=- 0 0 IN IP4 0.0.0.0\r\ns=airplay2-rs\r\nc=IN IP4 0.0.0.0\r\nt=0 0\r\nm=audio 0 RTP/AVP 96\r\na=rtpmap:96 AppleLossless\r\na=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n".to_string()
                }
                AudioCodec::Pcm => {
                    "v=0\r\no=- 0 0 IN IP4 0.0.0.0\r\ns=airplay2-rs\r\nc=IN IP4 0.0.0.0\r\nt=0 0\r\nm=audio 0 RTP/AVP 96\r\na=rtpmap:96 L16/44100/2\r\na=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n".to_string()
                }
                AudioCodec::Aac => {
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
            let announce_response = self.send_rtsp_request(&announce_req).await?;
            tracing::debug!(
                "ANNOUNCE response status: {}",
                announce_response.status.as_u16()
            );
        }

        // 4. Session Setup (SETUP Step 1: Info/Timing/Event)
        tracing::debug!("Performing Session SETUP (Step 1)...");
        let ek = self.encryption_key().await.unwrap_or([0u8; 32]);

        let eiv = {
            use rand::RngCore;
            let mut rng = rand::thread_rng();
            let mut iv = [0u8; 16];
            rng.fill_bytes(&mut iv);
            iv
        };

        // Determine timing protocol based on device capabilities
        // Devices supporting Buffered Audio (AirPlay 2) typically require/support PTP
        // Legacy devices use NTP.
        // Note: We reuse the `use_ptp` decision made earlier to ensure consistency
        // (e.g. skipping ANNOUNCE implies using PTP SETUP flow).

        let setup_plist_step1 = if use_ptp {
            tracing::info!("Device supports Buffered Audio - Using PTP timing protocol");

            // Get local IP from the connected stream if possible
            let local_ip = {
                let stream_guard = self.stream.lock().await;
                if let Some(ref stream) = *stream_guard {
                    stream.local_addr().ok().map(|a| a.ip().to_string())
                } else {
                    None
                }
            }
            .unwrap_or_else(|| "0.0.0.0".to_string());

            let timing_peer_info = DictBuilder::new()
                .insert("Addresses", vec![local_ip])
                .insert(
                    "ID",
                    self.rtsp_session
                        .lock()
                        .await
                        .as_ref()
                        .map(|s| s.client_session_id().to_string())
                        .unwrap_or_default(),
                )
                .build();

            DictBuilder::new()
                .insert("timingProtocol", "PTP")
                .insert("timingPeerInfo", timing_peer_info)
                .insert("groupUUID", group_uuid)
                .insert("macAddress", "AC:07:75:12:4A:1F")
                .insert("isAudioReceiver", false)
                .insert("ekey", ek.to_vec())
                .insert("eiv", eiv.to_vec())
                .insert("et", 4)
                .build()
        } else {
            tracing::info!("Device does not support Buffered Audio - Using NTP timing protocol");
            DictBuilder::new()
                .insert("timingProtocol", "NTP")
                .insert("ekey", ek.to_vec())
                .insert("eiv", eiv.to_vec())
                .insert("et", 4)
                .build()
        };

        let setup_req_step1 = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            // Per airplay2-homepod.md, SETUP #1 plist example doesn't show Transport header
            session.setup_session_request(&setup_plist_step1, None)
        };
        let response_step1 = self.send_rtsp_request(&setup_req_step1).await?;
        tracing::info!(
            "SETUP Step 1 response status: {}, body length: {} bytes",
            response_step1.status.as_u16(),
            response_step1.body.len()
        );
        if !response_step1.body.is_empty() {
            let hex_len = response_step1.body.len().min(256);
            tracing::info!(
                "SETUP Step 1 raw body (first {} bytes hex): {:02X?}",
                hex_len,
                &response_step1.body[..hex_len]
            );
        }

        // Parse Event/Timing ports from Step 1
        let (server_event_port, server_timing_port) =
            match crate::protocol::plist::decode(&response_step1.body) {
                Ok(plist) => {
                    tracing::info!("SETUP Step 1 plist: {:#?}", plist);
                    if let Some(dict) = plist.as_dict() {
                        let ep = dict
                            .get("eventPort")
                            .and_then(crate::protocol::plist::PlistValue::as_i64)
                            .and_then(|i| u16::try_from(i).ok());
                        let tp = dict
                            .get("timingPort")
                            .and_then(crate::protocol::plist::PlistValue::as_i64)
                            .and_then(|i| u16::try_from(i).ok());
                        tracing::info!(
                            "SETUP Step 1 ports: eventPort={:?}, timingPort={:?}",
                            ep,
                            tp
                        );
                        // Also log timingPeerInfo from device
                        if let Some(tpi) = dict.get("timingPeerInfo") {
                            tracing::info!("Device timingPeerInfo: {:#?}", tpi);
                        }
                        (ep, tp)
                    } else {
                        (None, None)
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to decode SETUP Step 1 plist: {}", e);
                    (None, None)
                }
            };

        // 5. Stream Setup (SETUP Step 2: Audio/Control)
        tracing::debug!("Performing Stream SETUP (Step 2)...");
        let audio_sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        let ctrl_sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        let time_sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;

        let audio_port = audio_sock.local_addr()?.port();
        let ctrl_port = ctrl_sock.local_addr()?.port();
        let time_port = time_sock.local_addr()?.port();

        tracing::debug!(
            "Bound local ports: Audio={}, Control={}, Timing={}",
            audio_port,
            ctrl_port,
            time_port
        );

        let transport = format!(
            "RTP/AVP/UDP;unicast;mode=record;client_port={audio_port};control_port={ctrl_port};timing_port={time_port}"
        );

        let stream_type = if self
            .device
            .read()
            .await
            .as_ref()
            .is_some_and(|d| d.capabilities.supports_buffered_audio)
        {
            96
        } else {
            100
        };

        let stream_entry = DictBuilder::new()
            .insert("type", stream_type)
            .insert("ct", 0x1) // Control Type (Audio)
            .insert("spf", 352) // Samples per frame (ALAC)
            .insert("audioType", "default")
            .insert("shk", ek.to_vec())
            .insert("controlPort", u64::from(ctrl_port))
            .insert("timingPort", u64::from(time_port))
            .build();

        let setup_plist_step2 = DictBuilder::new()
            .insert("streams", vec![stream_entry])
            .build();

        let setup_req_step2 = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            // Send transport header now to negotiate ports
            session.setup_session_request(&setup_plist_step2, Some(&transport))
        };
        let response_step2 = self.send_rtsp_request(&setup_req_step2).await?;
        tracing::info!(
            "SETUP Step 2 response status: {}, body length: {} bytes",
            response_step2.status.as_u16(),
            response_step2.body.len()
        );
        if !response_step2.body.is_empty() {
            let hex_len = response_step2.body.len().min(256);
            tracing::info!(
                "SETUP Step 2 raw body (first {} bytes hex): {:02X?}",
                hex_len,
                &response_step2.body[..hex_len]
            );
        }

        let mut server_ports = None;
        match crate::protocol::plist::decode(&response_step2.body) {
            Ok(plist) => {
                tracing::info!("SETUP Step 2 plist: {:#?}", plist);
                if let Some(dict) = plist.as_dict() {
                    // Try to find stream with dataPort/controlPort
                    // Or top level if they reply there
                    // Check top level first
                    let dp = dict
                        .get("dataPort")
                        .and_then(crate::protocol::plist::PlistValue::as_i64)
                        .and_then(|i| u16::try_from(i).ok());
                    let cp = dict
                        .get("controlPort")
                        .and_then(crate::protocol::plist::PlistValue::as_i64)
                        .and_then(|i| u16::try_from(i).ok());

                    // Also check inside 'streams' array if present
                    let stream_ports = if let Some(streams) = dict
                        .get("streams")
                        .and_then(crate::protocol::plist::PlistValue::as_array)
                    {
                        streams.first().and_then(|s| s.as_dict()).map(|d| {
                            (
                                d.get("dataPort")
                                    .and_then(crate::protocol::plist::PlistValue::as_i64)
                                    .and_then(|i| u16::try_from(i).ok()),
                                d.get("controlPort")
                                    .and_then(crate::protocol::plist::PlistValue::as_i64)
                                    .and_then(|i| u16::try_from(i).ok()),
                            )
                        })
                    } else {
                        None
                    };

                    let (data_port, control_port) = match (dp, cp) {
                        (Some(d), Some(c)) => (Some(d), Some(c)),
                        _ => stream_ports.unwrap_or((None, None)),
                    };

                    if let (Some(dp), Some(cp)) = (data_port, control_port) {
                        // We need event/timing ports too. Use ones from Step 1 or fallback to default/derived.
                        let ep = server_event_port.unwrap_or(0); // Sockets might fail if 0?
                        let tp = server_timing_port.unwrap_or(0);
                        server_ports = Some((dp, cp, ep, tp));
                    }
                }
            }
            Err(e) => tracing::warn!("Failed to decode SETUP Step 2 plist: {}", e),
        }

        // Check for Transport header in Step 2 response
        if server_ports.is_none() {
            if let Some(transport_header) = response_step2.headers.get("Transport") {
                if let Ok((sp, cp, tp)) = Self::parse_transport_ports(transport_header) {
                    // parse_transport_ports returns (server_port, control_port, timing_port)
                    // server_port is data port.
                    // timing_port is usually timing port.
                    // Where is event port? Only in plist?
                    // Use step 1 event port.
                    let ep = server_event_port.unwrap_or(0);
                    server_ports = Some((sp, cp, ep, tp));
                }
            }
        }

        if let Some((server_audio_port, server_ctrl_port, _server_event_port, server_time_port)) =
            server_ports
        {
            // Modified to accept 4 ports
            tracing::info!("Ports negotiated via SETUP sequence.");
            // Note: server_ports is now (audio, control, event, timing)

            tracing::info!(
                "Ports found in Session SETUP (Plist or Transport). Skipping Stream SETUP."
            );

            // Connect UDP sockets to server ports
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

            tracing::info!("Connecting Audio to {}:{}", device_ip, server_audio_port);
            tracing::info!("Connecting Control to {}:{}", device_ip, server_ctrl_port);
            tracing::info!("Connecting Timing to {}:{}", device_ip, server_time_port);

            audio_sock.connect((device_ip, server_audio_port)).await?;
            ctrl_sock.connect((device_ip, server_ctrl_port)).await?;
            time_sock.connect((device_ip, server_time_port)).await?;

            // 7b. Send SETPEERS and start PTP master handler if using PTP timing
            if use_ptp {
                // Send SETPEERS to tell device about PTP timing peers
                if let Err(e) = self.send_set_peers(device_ip).await {
                    tracing::warn!("SETPEERS failed (continuing anyway): {}", e);
                }

                self.start_ptp_master(&time_sock, device_ip, server_time_port)
                    .await;
            }

            *self.sockets.lock().await = Some(UdpSockets {
                audio: audio_sock,
                control: ctrl_sock,
                timing: time_sock,
                server_audio_port,
                server_control_port: server_ctrl_port,
                server_timing_port: server_time_port,
            });
        }

        // 8. Send RECORD to start the streaming session
        if use_ptp {
            // For AirPlay 2 Buffered Audio: RECORD is sent as part of the setup flow
            // (per protocol: SETUP → SETPEERS → RECORD → FLUSH → Audio data)
            tracing::info!("Sending RECORD for AirPlay 2 Buffered Audio...");
            match tokio::time::timeout(std::time::Duration::from_secs(5), self.record()).await {
                Ok(Ok(())) => tracing::info!("RECORD accepted by device"),
                Ok(Err(e)) => tracing::warn!("RECORD failed: {}", e),
                Err(_) => tracing::warn!("RECORD timed out after 5s (device may respond later)"),
            }
        }
        // Note: For NTP/AirPlay 1 devices, RECORD is still deferred until streaming starts.
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

    /// Send RECORD request to start buffering/playback
    ///
    /// # Errors
    ///
    /// Returns error if RTSP request fails
    /// Send SETPEERS to tell the device about PTP timing peers.
    /// This is required for `AirPlay` 2 PTP timing.
    async fn send_set_peers(&self, device_ip: std::net::IpAddr) -> Result<(), AirPlayError> {
        use crate::protocol::plist::PlistValue;

        // Get our local IP from the connected stream
        let local_ip = {
            let stream_guard = self.stream.lock().await;
            if let Some(ref stream) = *stream_guard {
                stream.local_addr().ok().map(|a| a.ip().to_string())
            } else {
                None
            }
        }
        .unwrap_or_else(|| "0.0.0.0".to_string());

        // Build peer list: array of IP address strings [our_ip, device_ip]
        let peer_list = PlistValue::Array(vec![
            PlistValue::String(local_ip.clone()),
            PlistValue::String(device_ip.to_string()),
        ]);

        let body =
            crate::protocol::plist::encode(&peer_list).map_err(|e| AirPlayError::RtspError {
                message: format!("Failed to encode SETPEERS plist: {e}"),
                status_code: None,
            })?;

        tracing::info!("Sending SETPEERS with peers: [{}, {}]", local_ip, device_ip);

        let request = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session.set_peers_request(body)
        };

        let response = self.send_rtsp_request(&request).await?;
        tracing::info!("SETPEERS response: {}", response.status.as_u16());
        Ok(())
    }

    /// Send RECORD command to start playback
    ///
    /// # Errors
    ///
    /// Returns error if RTSP request fails
    pub async fn record(&self) -> Result<(), AirPlayError> {
        tracing::debug!("Sending RECORD request...");
        let record_request = {
            let mut session_guard = self.rtsp_session.lock().await;
            let session = session_guard
                .as_mut()
                .ok_or_else(|| AirPlayError::InvalidState {
                    message: "No RTSP session".to_string(),
                    current_state: "None".to_string(),
                })?;
            session.record_request()
        };
        self.send_rtsp_request(&record_request).await?;
        Ok(())
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

        // Stop PTP handler if running
        self.stop_ptp().await;

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

    /// Determine if PTP should be used based on config and device capabilities.
    async fn should_use_ptp(&self) -> bool {
        match self.config.timing_protocol {
            TimingProtocol::Ptp => true,
            TimingProtocol::Ntp => false,
            TimingProtocol::Auto => {
                // Use PTP if the device supports it (AirPlay 2 devices)
                let device_guard = self.device.read().await;
                device_guard
                    .as_ref()
                    .is_some_and(|d| d.supports_ptp() || d.supports_airplay2())
            }
        }
    }

    /// Start the PTP slave handler as a background task.
    ///
    /// The `HomePod` acts as PTP grandmaster clock. We act as slave,
    /// syncing to the device's clock for accurate RTP timestamping.
    ///
    /// `AirPlay` 2 PTP uses standard IEEE 1588 ports:
    /// - Port 319 for event messages (Sync, `Delay_Req`)
    /// - Port 320 for general messages (`Follow_Up`, `Delay_Resp`)
    ///
    /// These are privileged ports requiring elevated/administrator access.
    /// If binding fails, PTP will not start — the device will not play audio.
    async fn start_ptp_master(
        &self,
        _timing_socket: &UdpSocket,
        device_ip: std::net::IpAddr,
        _server_timing_port: u16,
    ) {
        use crate::protocol::ptp::handler::{PTP_EVENT_PORT, PTP_GENERAL_PORT, PtpMasterHandler};

        let clock_id: u64 = rand::random();
        let clock = create_shared_clock(clock_id, PtpRole::Master);

        // Bind to standard PTP event port (319) — privileged, requires admin
        let ptp_event_socket = match UdpSocket::bind(("0.0.0.0", PTP_EVENT_PORT)).await {
            Ok(sock) => {
                tracing::info!("PTP event socket bound to port {}", PTP_EVENT_PORT);
                sock
            }
            Err(e) => {
                tracing::error!(
                    "Failed to bind PTP event port {} — run with elevated/admin privileges! Error: {}",
                    PTP_EVENT_PORT,
                    e
                );
                return;
            }
        };

        // Bind to standard PTP general port (320) — privileged, requires admin
        let ptp_general_socket = match UdpSocket::bind(("0.0.0.0", PTP_GENERAL_PORT)).await {
            Ok(sock) => {
                tracing::info!("PTP general socket bound to port {}", PTP_GENERAL_PORT);
                Some(Arc::new(sock))
            }
            Err(e) => {
                tracing::error!(
                    "Failed to bind PTP general port {} — run with elevated/admin privileges! Error: {}",
                    PTP_GENERAL_PORT,
                    e
                );
                return;
            }
        };

        let ptp_event_socket = Arc::new(ptp_event_socket);

        let config = PtpHandlerConfig {
            clock_id,
            role: PtpRole::Master,
            sync_interval: std::time::Duration::from_secs(1),
            delay_req_interval: std::time::Duration::from_secs(1),
            recv_buf_size: 256,
            use_airplay_format: false, // HomePod uses standard IEEE 1588 PTP (44-byte messages)
        };

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        // We are the PTP master — send Sync/Follow_Up to the HomePod (slave)
        let slave_addr = std::net::SocketAddr::new(device_ip, PTP_EVENT_PORT);
        let slave_general_addr = std::net::SocketAddr::new(device_ip, PTP_GENERAL_PORT);

        let handler_clock = clock.clone();

        tokio::spawn(async move {
            let mut handler =
                PtpMasterHandler::new(ptp_event_socket, ptp_general_socket, handler_clock, config);

            // Pre-populate with the HomePod as slave — don't wait for Delay_Req
            handler.add_slave(slave_addr);
            handler.add_general_slave(slave_general_addr);

            tracing::info!(
                "PTP master handler started (clock_id=0x{:016X}, slave={})",
                clock_id,
                slave_addr
            );
            if let Err(e) = handler.run(shutdown_rx).await {
                tracing::error!("PTP master handler error: {}", e);
            }
            tracing::info!("PTP master handler stopped");
        });

        *self.ptp_clock.lock().await = Some(clock);
        *self.ptp_shutdown_tx.lock().await = Some(shutdown_tx);
        *self.ptp_active.write().await = true;

        tracing::info!(
            "PTP timing started as MASTER for slave at {} (event port {}, general port {})",
            device_ip,
            PTP_EVENT_PORT,
            PTP_GENERAL_PORT
        );
    }

    /// Stop the PTP master handler if running.
    async fn stop_ptp(&self) {
        if let Some(tx) = self.ptp_shutdown_tx.lock().await.take() {
            let _ = tx.send(true);
            tracing::info!("PTP master handler shutdown signal sent");
        }
        *self.ptp_clock.lock().await = None;
        *self.ptp_active.write().await = false;
    }

    /// Get the shared PTP clock, if PTP timing is active.
    pub async fn ptp_clock(&self) -> Option<SharedPtpClock> {
        self.ptp_clock.lock().await.clone()
    }

    /// Check if PTP timing is active for the current connection.
    pub async fn is_ptp_active(&self) -> bool {
        *self.ptp_active.read().await
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
