use crate::protocol::rtp::raop_timing::{RaopTimingRequest, RaopTimingResponse, TimingSync};
use crate::protocol::rtp::timing::NtpTimestamp;

fn ntp_from_micros(micros: u64) -> NtpTimestamp {
    let seconds = (micros / 1_000_000) as u32;
    let micros_rem = micros % 1_000_000;
    let fraction = ((micros_rem as u64 * 4_294_967_296) / 1_000_000) as u32;
    NtpTimestamp { seconds, fraction }
}

#[test]
fn test_timing_request_encode() {
    let mut request = RaopTimingRequest::new();
    // Fix reference time for deterministic output
    request.reference_time = NtpTimestamp {
        seconds: 0x12345678,
        fraction: 0x9ABCDEF0,
    };

    let encoded = request.encode(12345);

    assert_eq!(encoded.len(), 32);
    // RTP Header: 80 D2
    assert_eq!(encoded[0], 0x80);
    assert_eq!(encoded[1], 0xD2);
    // Sequence: 12345 = 0x3039
    assert_eq!(encoded[2], 0x30);
    assert_eq!(encoded[3], 0x39);
    // Timestamp: 0
    assert_eq!(encoded[4..8], [0, 0, 0, 0]);
    // Reference time
    let ref_bytes = request.reference_time.encode();
    assert_eq!(encoded[8..16], ref_bytes);
    // Receive time (0)
    assert_eq!(encoded[16..24], [0; 8]);
    // Send time (same as reference)
    assert_eq!(encoded[24..32], ref_bytes);
}

#[test]
fn test_timing_response_decode() {
    // Construct a mock response
    let ref_time = NtpTimestamp {
        seconds: 100,
        fraction: 0,
    };
    let recv_time = NtpTimestamp {
        seconds: 100,
        fraction: 0x80000000,
    }; // +0.5s
    let send_time = NtpTimestamp {
        seconds: 101,
        fraction: 0,
    }; // +1.0s

    let mut buf = Vec::new();
    // Header (8 bytes dummy)
    buf.extend_from_slice(&[0; 8]);
    buf.extend_from_slice(&ref_time.encode());
    buf.extend_from_slice(&recv_time.encode());
    buf.extend_from_slice(&send_time.encode());

    let response = RaopTimingResponse::decode(&buf).expect("Decode failed");

    assert_eq!(response.reference_time.seconds, 100);
    assert_eq!(response.receive_time.fraction, 0x80000000);
    assert_eq!(response.send_time.seconds, 101);
}

#[test]
fn test_offset_calculation() {
    let t1 = NtpTimestamp {
        seconds: 100,
        fraction: 0,
    }; // 100.0
    let t2 = NtpTimestamp {
        seconds: 105,
        fraction: 0,
    }; // 105.0 (Client 100 -> Server 105, offset +5)
    let t3 = NtpTimestamp {
        seconds: 105,
        fraction: 0x80000000,
    }; // 105.5 (Server processing 0.5s)
    let t4 = NtpTimestamp {
        seconds: 101,
        fraction: 0,
    }; // 101.0 (RTT 1.0s total, 0.5s network)

    // Offset = ((t2 - t1) + (t3 - t4)) / 2
    // t2 - t1 = 5.0
    // t3 - t4 = 4.5
    // (5.0 + 4.5) / 2 = 4.75s = 4,750,000 us

    let response = RaopTimingResponse {
        reference_time: t1,
        receive_time: t2,
        send_time: t3,
    };

    let offset = response.calculate_offset(t4);
    assert_eq!(offset, 4_750_000);
}

#[test]
fn test_timing_sync_flow() {
    let mut sync = TimingSync::new();

    // 1. Create request
    let req_data = sync.create_request();
    // Extract sequence number from request to mock response
    let _seq = u16::from_be_bytes([req_data[2], req_data[3]]);

    // 2. Mock response
    let offset_us = 1_000_000; // 1 second offset
    let req_time = NtpTimestamp::decode(&req_data[8..16]);

    let server_recv = ntp_from_micros(req_time.to_micros() + offset_us);
    let server_send = server_recv; // Instant processing

    // Response construction
    let mut resp_data = Vec::new();
    // Header (RTP) - copy from request but change packet type/marker?
    // Actually decode skips header (8 bytes).
    resp_data.extend_from_slice(&[0x80, 0xD3, 0, 0, 0, 0, 0, 0]); // 8 bytes header (dummy)
    resp_data.extend_from_slice(&req_time.encode());
    resp_data.extend_from_slice(&server_recv.encode());
    resp_data.extend_from_slice(&server_send.encode());

    // 3. Process response
    sync.process_response(&resp_data).expect("Process response");

    let calculated = sync.offset();
    // See comments in previous version for logic
    assert!(calculated > 0);
    assert!(calculated <= 1_000_000);
}

#[test]
fn test_timestamp_conversion() {
    let sync = TimingSync::new();
    // Inject offset via reflection/internal mutation? No, public API only.
    // We can simulate a response that produces a known offset.
    // But without mocking time, it's hard to get exact offset.

    // However, we can test that local_to_remote and remote_to_local are inverse (approx)
    // regardless of the offset value (initially 0).

    let local = 1000;
    let remote = sync.local_to_remote(local);
    assert_eq!(remote, 1000); // Offset 0 initially

    let back = sync.remote_to_local(remote);
    assert_eq!(back, 1000);
}
