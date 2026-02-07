//! Mock `AirPlay` sender for testing the receiver
//!
//! Simulates an `AirPlay` sender (like iTunes) to test receiver functionality
//! without requiring real hardware or software.

use crate::net::{AsyncReadExt, AsyncWriteExt};
use crate::protocol::rtsp::{Headers, Method, RtspResponse};
use crate::receiver::timing::NtpTimestamp;
use std::fmt::Write;
use std::net::SocketAddr;
use tokio::net::{TcpStream, UdpSocket};

/// Mock sender configuration
#[derive(Debug, Clone)]
pub struct MockSenderConfig {
    /// Receiver address to connect to
    pub receiver_addr: SocketAddr,
    /// Audio codec to use
    pub codec: MockCodec,
    /// Enable encryption
    pub encrypted: bool,
    /// Sample rate
    pub sample_rate: u32,
    /// Frames per packet
    pub frames_per_packet: u32,
}

/// Audio codec for mock sender
#[derive(Debug, Clone, Copy)]
pub enum MockCodec {
    /// Apple Lossless
    Alac,
    /// Linear PCM
    Pcm,
    /// AAC
    Aac,
}

impl Default for MockSenderConfig {
    fn default() -> Self {
        Self {
            receiver_addr: "127.0.0.1:5000".parse().unwrap(),
            codec: MockCodec::Alac,
            encrypted: false,
            sample_rate: 44100,
            frames_per_packet: 352,
        }
    }
}

/// Mock `AirPlay` sender
pub struct MockSender {
    config: MockSenderConfig,
    rtsp_stream: Option<TcpStream>,
    audio_socket: Option<UdpSocket>,
    control_socket: Option<UdpSocket>,
    timing_socket: Option<UdpSocket>,
    cseq: u32,
    session_id: Option<String>,
    server_ports: Option<ServerPorts>,
    sequence: u16,
    timestamp: u32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ServerPorts {
    audio: u16,
    control: u16,
    timing: u16,
}

impl MockSender {
    /// Create a new mock sender
    #[must_use]
    pub fn new(config: MockSenderConfig) -> Self {
        Self {
            config,
            rtsp_stream: None,
            audio_socket: None,
            control_socket: None,
            timing_socket: None,
            cseq: 0,
            session_id: None,
            server_ports: None,
            sequence: 0,
            timestamp: 0,
        }
    }

    /// Connect to receiver
    ///
    /// # Errors
    /// Returns `MockSenderError` if connection fails.
    pub async fn connect(&mut self) -> Result<(), MockSenderError> {
        let stream = TcpStream::connect(self.config.receiver_addr).await?;
        self.rtsp_stream = Some(stream);
        Ok(())
    }

    /// Perform OPTIONS request
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn options(&mut self) -> Result<RtspResponse, MockSenderError> {
        self.send_rtsp_request(Method::Options, "*", None).await
    }

    /// Perform ANNOUNCE with SDP
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn announce(&mut self) -> Result<RtspResponse, MockSenderError> {
        let sdp = self.build_sdp();
        let uri = format!("rtsp://{}/1234", self.config.receiver_addr);

        self.send_rtsp_request(
            Method::Announce,
            &uri,
            Some(("application/sdp", sdp.as_bytes())),
        )
        .await
    }

    /// Perform SETUP
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn setup(&mut self) -> Result<RtspResponse, MockSenderError> {
        // Bind local UDP sockets
        let audio_socket = UdpSocket::bind("0.0.0.0:0").await?;
        let control_socket = UdpSocket::bind("0.0.0.0:0").await?;
        let timing_socket = UdpSocket::bind("0.0.0.0:0").await?;

        let control_port = control_socket.local_addr()?.port();
        let timing_port = timing_socket.local_addr()?.port();

        let transport = format!(
            "RTP/AVP/UDP;unicast;mode=record;control_port={control_port};timing_port={timing_port}",
        );

        let uri = format!("rtsp://{}/1234", self.config.receiver_addr);

        let response = self
            .send_rtsp_request_with_headers(
                Method::Setup,
                &uri,
                vec![("Transport", &transport)],
                None,
            )
            .await?;

        // Parse response for server ports and session
        if response.status.0 == 200 {
            self.session_id = response.headers.get("Session").map(ToString::to_string);
            self.server_ports = Self::parse_transport(&response);

            self.audio_socket = Some(audio_socket);
            self.control_socket = Some(control_socket);
            self.timing_socket = Some(timing_socket);

            // Connect audio socket to server
            if let Some(ref ports) = self.server_ports {
                let server_audio = SocketAddr::new(self.config.receiver_addr.ip(), ports.audio);
                if let Some(ref socket) = self.audio_socket {
                    socket.connect(server_audio).await?;
                }
            }
        }

        Ok(response)
    }

    /// Perform RECORD
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn record(&mut self) -> Result<RtspResponse, MockSenderError> {
        let uri = format!("rtsp://{}/1234", self.config.receiver_addr);

        self.send_rtsp_request_with_headers(
            Method::Record,
            &uri,
            vec![
                ("Range", "npt=0-"),
                (
                    "RTP-Info",
                    &format!("seq={};rtptime={}", self.sequence, self.timestamp),
                ),
            ],
            None,
        )
        .await
    }

    /// Send an audio packet
    ///
    /// # Errors
    /// Returns `MockSenderError` if send fails.
    #[allow(clippy::cast_possible_truncation)]
    pub async fn send_audio(&mut self, audio_data: &[u8]) -> Result<(), MockSenderError> {
        let socket = self
            .audio_socket
            .as_ref()
            .ok_or(MockSenderError::NotSetup)?;

        // Build RTP packet
        let mut packet = vec![
            0x80,
            0x60, // V=2, PT=96
            (self.sequence >> 8) as u8,
            self.sequence as u8,
            (self.timestamp >> 24) as u8,
            (self.timestamp >> 16) as u8,
            (self.timestamp >> 8) as u8,
            self.timestamp as u8,
            0x12,
            0x34,
            0x56,
            0x78, // SSRC
        ];
        packet.extend_from_slice(audio_data);

        socket.send(&packet).await?;

        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self
            .timestamp
            .wrapping_add(self.config.frames_per_packet);

        Ok(())
    }

    /// Send a sync packet
    ///
    /// # Errors
    /// Returns `MockSenderError` if send fails.
    pub async fn send_sync(&mut self) -> Result<(), MockSenderError> {
        let socket = self
            .control_socket
            .as_ref()
            .ok_or(MockSenderError::NotSetup)?;

        let ports = self
            .server_ports
            .as_ref()
            .ok_or(MockSenderError::NotSetup)?;

        let server_control = SocketAddr::new(self.config.receiver_addr.ip(), ports.control);

        // Build sync packet
        let now_ntp = NtpTimestamp::now();
        let mut packet = vec![
            0x90, 0xD4, // Sync packet type
            0x00, 0x00, // Sequence (unused)
        ];

        // RTP timestamp
        packet.extend_from_slice(&self.timestamp.to_be_bytes());

        // NTP timestamp
        packet.extend_from_slice(&now_ntp.to_u64().to_be_bytes());

        // RTP timestamp at NTP
        packet.extend_from_slice(&self.timestamp.to_be_bytes());

        socket.send_to(&packet, server_control).await?;

        Ok(())
    }

    /// Perform TEARDOWN
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn teardown(&mut self) -> Result<RtspResponse, MockSenderError> {
        let uri = format!("rtsp://{}/1234", self.config.receiver_addr);
        self.send_rtsp_request(Method::Teardown, &uri, None).await
    }

    /// Set volume
    ///
    /// # Errors
    /// Returns `MockSenderError` if request fails.
    pub async fn set_volume(&mut self, db: f32) -> Result<RtspResponse, MockSenderError> {
        let uri = format!("rtsp://{}/1234", self.config.receiver_addr);
        let body = format!("volume: {db:.6}\r\n");

        self.send_rtsp_request_with_headers(
            Method::SetParameter,
            &uri,
            vec![("Content-Type", "text/parameters")],
            Some(("text/parameters", body.as_bytes())),
        )
        .await
    }

    // Helper methods

    fn build_sdp(&self) -> String {
        let codec_name = match self.config.codec {
            MockCodec::Alac => "AppleLossless",
            MockCodec::Pcm => "L16",
            MockCodec::Aac => "mpeg4-generic",
        };

        format!(
            "v=0\r\n\
             o=iTunes 0 0 IN IP4 {}\r\n\
             s=iTunes\r\n\
             c=IN IP4 {}\r\n\
             t=0 0\r\n\
             m=audio 0 RTP/AVP 96\r\n\
             a=rtpmap:96 {}\r\n\
             a=fmtp:96 {} 0 16 40 10 14 2 255 0 0 {}\r\n",
            self.config.receiver_addr.ip(),
            self.config.receiver_addr.ip(),
            codec_name,
            self.config.frames_per_packet,
            self.config.sample_rate,
        )
    }

    async fn send_rtsp_request(
        &mut self,
        method: Method,
        uri: &str,
        body: Option<(&str, &[u8])>,
    ) -> Result<RtspResponse, MockSenderError> {
        self.send_rtsp_request_with_headers(method, uri, vec![], body)
            .await
    }

    async fn send_rtsp_request_with_headers(
        &mut self,
        method: Method,
        uri: &str,
        headers: Vec<(&str, &str)>,
        body: Option<(&str, &[u8])>,
    ) -> Result<RtspResponse, MockSenderError> {
        let stream = self
            .rtsp_stream
            .as_mut()
            .ok_or(MockSenderError::NotConnected)?;

        self.cseq += 1;

        // Build request
        let mut request = String::new();
        let _ = write!(request, "{} {} RTSP/1.0\r\n", method.as_str(), uri);
        let _ = write!(request, "CSeq: {}\r\n", self.cseq);

        if let Some(ref session) = self.session_id {
            let _ = write!(request, "Session: {session}\r\n");
        }

        for (name, value) in headers {
            let _ = write!(request, "{name}: {value}\r\n");
        }

        if let Some((content_type, data)) = body {
            let _ = write!(request, "Content-Type: {content_type}\r\n");
            let _ = write!(request, "Content-Length: {}\r\n", data.len());
            request.push_str("\r\n");
            stream.write_all(request.as_bytes()).await?;
            stream.write_all(data).await?;
        } else {
            request.push_str("\r\n");
            stream.write_all(request.as_bytes()).await?;
        }

        // Read response
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await?;

        Self::parse_response(&buf[..n])
    }

    fn parse_response(data: &[u8]) -> Result<RtspResponse, MockSenderError> {
        let text = String::from_utf8_lossy(data);
        let mut lines = text.lines();

        // Status line
        let status_line = lines.next().ok_or(MockSenderError::InvalidResponse)?;

        let parts: Vec<&str> = status_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(MockSenderError::InvalidResponse);
        }

        let status_code: u16 = parts[1]
            .parse()
            .map_err(|_| MockSenderError::InvalidResponse)?;

        // Headers
        let mut headers = Headers::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some(pos) = line.find(':') {
                headers.insert(
                    line[..pos].trim().to_string(),
                    line[pos + 1..].trim().to_string(),
                );
            }
        }

        Ok(RtspResponse {
            version: "RTSP/1.0".to_string(),
            status: crate::protocol::rtsp::StatusCode(status_code),
            reason: "OK".to_string(), // Simplified reason phrase
            headers,
            body: Vec::new(),
        })
    }

    fn parse_transport(response: &RtspResponse) -> Option<ServerPorts> {
        let transport = response.headers.get("Transport")?;

        let mut audio = 0u16;
        let mut control = 0u16;
        let mut timing = 0u16;

        for part in transport.split(';') {
            if let Some(value) = part.strip_prefix("server_port=") {
                if let Some(port_str) = value.split('-').next() {
                    audio = port_str.parse().unwrap_or(0);
                }
            }
            if let Some(value) = part.strip_prefix("control_port=") {
                control = value.parse().unwrap_or(0);
            }
            if let Some(value) = part.strip_prefix("timing_port=") {
                timing = value.parse().unwrap_or(0);
            }
        }

        if audio > 0 && control > 0 && timing > 0 {
            Some(ServerPorts {
                audio,
                control,
                timing,
            })
        } else {
            None
        }
    }
}

/// Errors from mock sender
#[derive(Debug, thiserror::Error)]
pub enum MockSenderError {
    /// Not connected to receiver
    #[error("Not connected")]
    NotConnected,

    /// Session not setup
    #[error("Not setup")]
    NotSetup,

    /// Invalid RTSP response
    #[error("Invalid response")]
    InvalidResponse,

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
