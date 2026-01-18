use super::*;

mod packet_tests {
    use super::*;

    #[test]
    fn test_header_encode_decode() {
        let header = RtpHeader::new_audio(100, 44100, 0x12345678, false);

        let encoded = header.encode();
        let decoded = RtpHeader::decode(&encoded).unwrap();

        assert_eq!(decoded.version, 2);
        assert_eq!(decoded.sequence, 100);
        assert_eq!(decoded.timestamp, 44100);
        assert_eq!(decoded.ssrc, 0x12345678);
        assert!(decoded.marker);
    }

    #[test]
    fn test_packet_encode_decode() {
        let payload = vec![0x01, 0x02, 0x03, 0x04];
        let packet = RtpPacket::audio(1, 352, 0xAABBCCDD, payload.clone(), false);

        let encoded = packet.encode();
        let decoded = RtpPacket::decode(&encoded).unwrap();

        assert_eq!(decoded.header.sequence, 1);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_payload_type_values() {
        assert_eq!(PayloadType::TimingRequest as u8, 0x52);
        assert_eq!(PayloadType::AudioRealtime as u8, 0x60);
    }

    #[test]
    fn test_decode_invalid_version() {
        let mut buf = [0u8; 12];
        buf[0] = 0x00; // Version 0 instead of 2

        let result = RtpHeader::decode(&buf);
        use crate::protocol::rtp::packet::RtpDecodeError;
        assert!(matches!(result, Err(RtpDecodeError::InvalidVersion(0))));
    }

    #[test]
    fn test_audio_samples_iterator() {
        let payload = vec![
            0x00, 0x01, 0x02, 0x03, // Sample 1: L=0x0100, R=0x0302
            0x04, 0x05, 0x06, 0x07, // Sample 2: L=0x0504, R=0x0706
        ];
        let packet = RtpPacket::audio(0, 0, 0, payload, false);

        let samples: Vec<_> = packet.audio_samples().collect();

        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0], (0x0100, 0x0302));
        assert_eq!(samples[1], (0x0504, 0x0706));
    }

    #[test]
    fn test_packet_buffer_too_small() {
        let buf = [0u8; 5];
        let result = RtpHeader::decode(&buf);
        assert!(matches!(
            result,
            Err(crate::protocol::rtp::packet::RtpDecodeError::BufferTooSmall { .. })
        ));
    }
}

mod timing_tests {
    use crate::protocol::rtp::timing::{NtpTimestamp, TimingRequest, TimingResponse};

    #[test]
    fn test_ntp_timestamp_encode_decode() {
        let ts = NtpTimestamp {
            seconds: 1234567890,
            fraction: 0x80000000,
        };

        let encoded = ts.encode();
        let decoded = NtpTimestamp::decode(&encoded);

        assert_eq!(decoded.seconds, ts.seconds);
        assert_eq!(decoded.fraction, ts.fraction);
    }

    #[test]
    fn test_ntp_timestamp_now() {
        let ts = NtpTimestamp::now();

        // Should be somewhere reasonable (after 2020)
        assert!(ts.seconds > 3786825600); // 2020-01-01 in NTP time
    }

    #[test]
    fn test_timing_request_encode() {
        let request = TimingRequest::new();
        let encoded = request.encode(1, 0x12345678);

        // Check header
        assert_eq!(encoded[0], 0x80); // V=2
        assert_eq!(encoded[1], 0xD2); // M=1, PT=0x52

        // Should be 40 bytes total (12 header + 4 padding + 24 timestamps)
        assert_eq!(encoded.len(), 40);
    }

    #[test]
    fn test_rtt_calculation() {
        // Simulate a response where server adds 10ms processing time
        let t1 = NtpTimestamp {
            seconds: 100,
            fraction: 0,
        };
        let t2 = NtpTimestamp {
            seconds: 100,
            fraction: 0x028F5C28,
        }; // +10ms
        let t3 = NtpTimestamp {
            seconds: 100,
            fraction: 0x051EB851,
        }; // +20ms
        let t4 = NtpTimestamp {
            seconds: 100,
            fraction: 0x0A3D70A3,
        }; // +40ms

        let response = TimingResponse {
            reference_time: t1,
            receive_time: t2,
            send_time: t3,
        };

        let rtt = response.calculate_rtt(t4);

        // RTT = (40-0) - (20-10) = 40 - 10 = 30ms ≈ 30000 microseconds
        // Allow some tolerance for floating point
        assert!(rtt > 25000 && rtt < 35000, "RTT was {}", rtt);
    }

    #[test]
    fn test_offset_calculation() {
        // Simulate clock skew
        // Client time: 100.000
        // Server time: 105.000 (offset +5s)

        // T1 (Client send): 100.000
        // T2 (Server recv): 105.010 (+5s + 10ms delay)
        // T3 (Server send): 105.020 (+5s + 20ms delay)
        // T4 (Client recv): 100.040 (+40ms delay)

        let t1 = NtpTimestamp {
            seconds: 100,
            fraction: 0,
        };
        let t2 = NtpTimestamp {
            seconds: 105,
            fraction: 0x028F5C28,
        }; // 10ms
        let t3 = NtpTimestamp {
            seconds: 105,
            fraction: 0x051EB851,
        }; // 20ms
        let t4 = NtpTimestamp {
            seconds: 100,
            fraction: 0x0A3D70A3,
        }; // 40ms

        let response = TimingResponse {
            reference_time: t1,
            receive_time: t2,
            send_time: t3,
        };

        let offset = response.calculate_offset(t4);
        // ((105.010 - 100.000) + (105.020 - 100.040)) / 2
        // (5.010 + 4.980) / 2 = 9.990 / 2 = 4.995s = 4995000us

        let expected = 4_995_000;
        let tolerance = 5_000; // 5ms tolerance

        assert!(
            (offset - expected).abs() < tolerance,
            "Offset was {}",
            offset
        );
    }
}

mod codec_tests {
    use super::*;

    #[test]
    fn test_codec_sequence_increment() {
        let mut codec = RtpCodec::new(0x12345678);

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let audio = vec![0u8; frame_size];

        let _ = codec.encode_audio(&audio).unwrap();
        assert_eq!(codec.sequence(), 1);

        let _ = codec.encode_audio(&audio).unwrap();
        assert_eq!(codec.sequence(), 2);
    }

    #[test]
    fn test_codec_timestamp_increment() {
        let mut codec = RtpCodec::new(0x12345678);

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let audio = vec![0u8; frame_size];

        let _ = codec.encode_audio(&audio).unwrap();
        assert_eq!(codec.timestamp(), 352);

        let _ = codec.encode_audio(&audio).unwrap();
        assert_eq!(codec.timestamp(), 704);
    }

    #[test]
    fn test_codec_invalid_audio_size() {
        let mut codec = RtpCodec::new(0);
        let audio = vec![0u8; 100]; // Wrong size

        let result = codec.encode_audio(&audio);
        assert!(matches!(result, Err(RtpCodecError::InvalidAudioSize(100))));
    }

    #[test]
    fn test_codec_encode_multiple_frames() {
        let mut codec = RtpCodec::new(0);

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let audio = vec![0u8; frame_size * 3]; // 3 frames

        let packets = codec.encode_audio_frames(&audio).unwrap();

        assert_eq!(packets.len(), 3);
        assert_eq!(codec.sequence(), 3);
    }

    #[test]
    fn test_codec_with_encryption() {
        let mut codec = RtpCodec::new(0);
        let key = [0x42u8; 16];
        let iv = [0x00u8; 16];
        codec.set_encryption(key, iv);

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let audio = vec![0xAA; frame_size];

        let packet = codec.encode_audio(&audio).unwrap();

        // Encrypted payload should differ from original
        let decoded = RtpPacket::decode(&packet).unwrap();
        assert_ne!(decoded.payload, audio);
    }

    #[test]
    fn test_codec_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 16];
        let iv = [0x00u8; 16];

        let mut encoder = RtpCodec::new(0x12345678);
        encoder.set_encryption(key, iv);

        let _decoder = RtpCodec::new(0x12345678);
        // Note: decoder needs same keys for decryption

        let frame_size = RtpCodec::FRAMES_PER_PACKET as usize * 4;
        let original = vec![0xAA; frame_size];

        let packet = encoder.encode_audio(&original).unwrap();

        // Create decoder with same keys
        let mut decoder = RtpCodec::new(0);
        decoder.set_encryption(key, iv);

        let decoded = decoder.decode_audio(&packet).unwrap();
        assert_eq!(decoded.payload, original);
    }

    #[test]
    fn test_packet_builder() {
        let builder = AudioPacketBuilder::new(0x1234);
        let packets = builder.add_audio(&vec![0u8; 352 * 4]).unwrap().build();

        assert_eq!(packets.len(), 1);
    }
}
