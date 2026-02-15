use crate::protocol::rtp::timing::{NtpTimestamp, TimingRequest, TimingResponse};

#[test]
fn test_ntp_timestamp_encode_decode() {
    let ts = NtpTimestamp {
        seconds: 1_234_567_890,
        fraction: 0x8000_0000,
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
    assert!(ts.seconds > 3_786_825_600); // 2020-01-01 in NTP time
}

#[test]
fn test_timing_request_encode() {
    let request = TimingRequest::new();
    let encoded = request.encode(1, 0x1234_5678);

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
        fraction: 0x028F_5C28,
    }; // +10ms
    let t3 = NtpTimestamp {
        seconds: 100,
        fraction: 0x051E_B851,
    }; // +20ms
    let t4 = NtpTimestamp {
        seconds: 100,
        fraction: 0x0A3D_70A3,
    }; // +40ms

    let response = TimingResponse {
        reference_time: t1,
        receive_time: t2,
        send_time: t3,
    };

    let rtt = response.calculate_rtt(t4);

    // RTT = (40-0) - (20-10) = 40 - 10 = 30ms ≈ 30000 microseconds
    // Allow some tolerance for floating point
    assert!(rtt > 25000 && rtt < 35000, "RTT was {rtt}");
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
        fraction: 0x028F_5C28,
    }; // 10ms
    let t3 = NtpTimestamp {
        seconds: 105,
        fraction: 0x051E_B851,
    }; // 20ms
    let t4 = NtpTimestamp {
        seconds: 100,
        fraction: 0x0A3D_70A3,
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

    assert!((offset - expected).abs() < tolerance, "Offset was {offset}",);
}
