//! RAOP unit test collection

mod discovery_tests {
    use airplay2::discovery::raop::*;
    use airplay2::types::{RaopCapabilities, RaopCodec};
    use std::collections::HashMap;

    #[test]
    fn test_parse_raop_capabilities() {
        let mut records = HashMap::new();
        records.insert("ch".to_string(), "2".to_string());
        records.insert("cn".to_string(), "0,1,2".to_string());
        records.insert("et".to_string(), "0,1".to_string());
        records.insert("sr".to_string(), "44100".to_string());

        let caps = RaopCapabilities::from_txt_records(&records);

        assert_eq!(caps.channels, 2);
        assert_eq!(caps.sample_rate, 44100);
        assert!(caps.supports_codec(RaopCodec::Alac));
        assert!(caps.supports_rsa());
    }

    #[test]
    fn test_service_name_parsing() {
        let (mac, name) = parse_raop_service_name("AABBCCDDEEFF@Living Room").unwrap();
        assert_eq!(mac, "AABBCCDDEEFF");
        assert_eq!(name, "Living Room");
    }
}

#[cfg(feature = "raop")]
mod crypto_tests {
    use airplay2::protocol::crypto::{RaopRsaPrivateKey, rsa_sizes as sizes};
    use rsa::traits::PublicKeyParts;

    #[test]
    fn test_rsa_key_generation() {
        let key = RaopRsaPrivateKey::generate().unwrap();
        let public = key.public_key();
        assert_eq!(public.size(), sizes::MODULUS_BYTES);
    }

    #[test]
    fn test_oaep_roundtrip() {
        let private = RaopRsaPrivateKey::generate().unwrap();
        let public = private.public_key();

        // Encrypt with public
        use airplay2::protocol::crypto::CompatibleOsRng;
        use rand::rngs::OsRng;
        use rsa::Oaep;
        use sha1::Sha1;

        let plaintext = b"test AES key 16b";
        let padding = Oaep::<Sha1>::new();
        let mut rng = CompatibleOsRng(OsRng);
        let encrypted = public.encrypt(&mut rng, padding, plaintext).unwrap();

        // Decrypt with private
        let decrypted = private.decrypt_oaep(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}

#[cfg(feature = "raop")]
mod sdp_tests {
    use airplay2::protocol::sdp::*;

    #[test]
    fn test_parse_raop_sdp() {
        let sdp_text = r#"v=0
o=iTunes 1234567890 1 IN IP4 192.168.1.100
s=iTunes
c=IN IP4 192.168.1.50
t=0 0
m=audio 0 RTP/AVP 96
a=rtpmap:96 AppleLossless
a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100
a=rsaaeskey:AAAA
a=aesiv:BBBB
"#;

        let sdp = SdpParser::parse(sdp_text).unwrap();
        assert_eq!(sdp.rsaaeskey(), Some("AAAA"));
        assert_eq!(sdp.aesiv(), Some("BBBB"));
    }

    #[test]
    fn test_build_announce_sdp() {
        let sdp = create_raop_announce_sdp(
            "1234567890",
            "192.168.1.100",
            "192.168.1.50",
            "encrypted_key",
            "init_vector",
        );

        assert!(sdp.contains("v=0"));
        assert!(sdp.contains("rsaaeskey:encrypted_key"));
        assert!(sdp.contains("aesiv:init_vector"));
    }
}

#[cfg(feature = "raop")]
mod encryption_tests {
    use airplay2::protocol::raop::encryption::*;

    #[test]
    fn test_aes_ctr_roundtrip() {
        let key = [0x42u8; 16];
        let iv = [0x00u8; 16];

        let encryptor = RaopEncryptor::new(key, iv);
        let decryptor = RaopDecryptor::new(key, iv);

        let original = vec![0xAA; FRAME_SIZE];
        let encrypted = encryptor.encrypt(&original, 0).unwrap();
        let decrypted = decryptor.decrypt(&encrypted, 0).unwrap();

        assert_eq!(decrypted, original);
    }
}

#[cfg(feature = "raop")]
mod rtp_tests {
    use airplay2::protocol::rtp::raop::*;

    #[test]
    fn test_audio_packet_roundtrip() {
        let payload = vec![1, 2, 3, 4, 5];
        let packet = RaopAudioPacket::new(100, 44100, 0x12345678, payload.clone()).with_marker();

        let encoded = packet.encode();
        let decoded = RaopAudioPacket::decode(&encoded).unwrap();

        assert_eq!(decoded.sequence, 100);
        assert_eq!(decoded.timestamp, 44100);
        assert!(decoded.marker);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_sync_packet() {
        use airplay2::protocol::rtp::NtpTimestamp;

        let ntp = NtpTimestamp::now();
        let packet = SyncPacket::new(1000, ntp, 1352, true);

        let encoded = packet.encode();
        let decoded = SyncPacket::decode(&encoded).unwrap();

        assert_eq!(decoded.rtp_timestamp, 1000);
        assert_eq!(decoded.next_timestamp, 1352);
        assert!(decoded.extension);
    }
}

#[cfg(feature = "raop")]
mod progress_tests {
    use airplay2::protocol::daap::DmapProgress;

    #[test]
    fn test_progress_encode_parse() {
        let progress = DmapProgress::new(1000, 2000, 10000);
        let encoded = progress.encode();
        let parsed = DmapProgress::parse(&encoded).unwrap();

        assert_eq!(parsed.start, 1000);
        assert_eq!(parsed.current, 2000);
        assert_eq!(parsed.end, 10000);
    }

    #[test]
    fn test_progress_percentage() {
        let progress = DmapProgress::new(0, 500, 1000);
        assert!((progress.percentage() - 0.5).abs() < 0.001);
    }
}

#[cfg(feature = "raop")]
mod session_tests {
    use airplay2::protocol::raop::session::*;

    #[test]
    fn test_session_state_machine() {
        let mut session = RaopRtspSession::new("192.168.1.50", 5000);
        assert_eq!(session.state(), RaopSessionState::Init);

        // Create OPTIONS request
        let request = session.options_request();
        // The mock logic says if Apple-Challenge is present. Client should send it.
        // Let's verify it is there.
        // We need to inspect headers. `Headers` in `rtsp` module has `get`.
        assert!(request.headers.get("Apple-Challenge").is_some());
    }
}
