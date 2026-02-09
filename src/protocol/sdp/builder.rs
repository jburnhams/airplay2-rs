use super::{MediaDescription, SdpConnection, SdpOrigin, SessionDescription};
use std::collections::HashMap;

/// Builder for SDP session descriptions
pub struct SdpBuilder {
    sdp: SessionDescription,
    current_media: Option<MediaDescription>,
}

impl Default for SdpBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SdpBuilder {
    /// Create a new SDP builder
    #[must_use]
    pub fn new() -> Self {
        Self {
            sdp: SessionDescription {
                version: 0,
                ..Default::default()
            },
            current_media: None,
        }
    }

    /// Set origin
    #[must_use]
    pub fn origin(
        mut self,
        username: &str,
        session_id: &str,
        session_version: &str,
        addr: &str,
    ) -> Self {
        self.sdp.origin = Some(SdpOrigin {
            username: username.to_string(),
            session_id: session_id.to_string(),
            session_version: session_version.to_string(),
            net_type: "IN".to_string(),
            addr_type: if addr.contains(':') { "IP6" } else { "IP4" }.to_string(),
            unicast_address: addr.to_string(),
        });
        self
    }

    /// Set session name
    #[must_use]
    pub fn session_name(mut self, name: &str) -> Self {
        self.sdp.session_name = name.to_string();
        self
    }

    /// Set connection info
    #[must_use]
    pub fn connection(mut self, addr: &str) -> Self {
        self.sdp.connection = Some(SdpConnection {
            net_type: "IN".to_string(),
            addr_type: if addr.contains(':') { "IP6" } else { "IP4" }.to_string(),
            address: addr.to_string(),
        });
        self
    }

    /// Set timing (usually 0 0 for live streams)
    #[must_use]
    pub fn timing(mut self, start: u64, stop: u64) -> Self {
        self.sdp.timing = Some((start, stop));
        self
    }

    /// Add session-level attribute
    #[must_use]
    pub fn attribute(mut self, name: &str, value: Option<&str>) -> Self {
        self.sdp
            .attributes
            .insert(name.to_string(), value.map(String::from));
        self
    }

    /// Start a media section
    #[must_use]
    pub fn media(mut self, media_type: &str, port: u16, protocol: &str, formats: &[&str]) -> Self {
        // Save previous media if any
        if let Some(media) = self.current_media.take() {
            self.sdp.media.push(media);
        }

        self.current_media = Some(MediaDescription {
            media_type: media_type.to_string(),
            port,
            protocol: protocol.to_string(),
            formats: formats.iter().map(ToString::to_string).collect(),
            attributes: HashMap::new(),
        });

        self
    }

    /// Add media-level attribute
    #[must_use]
    pub fn media_attribute(mut self, name: &str, value: Option<&str>) -> Self {
        if let Some(ref mut media) = self.current_media {
            media
                .attributes
                .insert(name.to_string(), value.map(String::from));
        }
        self
    }

    /// Build the SDP
    #[must_use]
    pub fn build(mut self) -> SessionDescription {
        // Save last media
        if let Some(media) = self.current_media.take() {
            self.sdp.media.push(media);
        }
        self.sdp
    }

    /// Build and encode as string
    #[must_use]
    pub fn encode(self) -> String {
        let sdp = self.build();
        encode_sdp(&sdp)
    }
}

/// Encode SDP to string format
#[must_use]
pub fn encode_sdp(sdp: &SessionDescription) -> String {
    use std::fmt::Write;
    let mut output = String::new();

    // Version
    write!(output, "v={}\r\n", sdp.version).unwrap();

    // Origin
    if let Some(ref o) = sdp.origin {
        write!(
            output,
            "o={} {} {} {} {} {}\r\n",
            o.username, o.session_id, o.session_version, o.net_type, o.addr_type, o.unicast_address
        )
        .unwrap();
    }

    // Session name
    write!(output, "s={}\r\n", sdp.session_name).unwrap();

    // Connection
    if let Some(ref c) = sdp.connection {
        write!(output, "c={} {} {}\r\n", c.net_type, c.addr_type, c.address).unwrap();
    }

    // Timing
    if let Some((start, stop)) = sdp.timing {
        write!(output, "t={start} {stop}\r\n").unwrap();
    }

    // Session attributes
    for (name, value) in &sdp.attributes {
        if let Some(v) = value {
            write!(output, "a={name}:{v}\r\n").unwrap();
        } else {
            write!(output, "a={name}\r\n").unwrap();
        }
    }

    // Media sections
    for media in &sdp.media {
        write!(
            output,
            "m={} {} {} {}\r\n",
            media.media_type,
            media.port,
            media.protocol,
            media.formats.join(" ")
        )
        .unwrap();

        for (name, value) in &media.attributes {
            if let Some(v) = value {
                write!(output, "a={name}:{v}\r\n").unwrap();
            } else {
                write!(output, "a={name}\r\n").unwrap();
            }
        }
    }

    output
}

/// Create RAOP ANNOUNCE SDP for Apple Lossless audio
#[must_use]
pub fn create_raop_announce_sdp(
    session_id: &str,
    client_ip: &str,
    server_ip: &str,
    rsaaeskey: &str,
    aesiv: &str,
) -> String {
    SdpBuilder::new()
        .origin("iTunes", session_id, "1", client_ip)
        .session_name("iTunes")
        .connection(server_ip)
        .timing(0, 0)
        .media("audio", 0, "RTP/AVP", &["96"])
        .media_attribute("rtpmap", Some("96 AppleLossless"))
        .media_attribute("fmtp", Some("96 352 0 16 40 10 14 2 255 0 0 44100"))
        .media_attribute("rsaaeskey", Some(rsaaeskey))
        .media_attribute("aesiv", Some(aesiv))
        .media_attribute("min-latency", Some("11025"))
        .encode()
}
