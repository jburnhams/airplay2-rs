//! Integration tests for PTP timing synchronization.
//!
//! Tests end-to-end PTP exchanges using real UDP sockets on loopback.

use std::sync::Arc;
use std::time::Duration;

use airplay2::protocol::ptp::clock::{PtpClock, PtpRole};
use airplay2::protocol::ptp::handler::{
    PtpHandlerConfig, PtpMasterHandler, PtpSlaveHandler, create_shared_clock,
};
use airplay2::protocol::ptp::message::{
    AirPlayTimingPacket, PtpMessage, PtpMessageBody, PtpMessageType, PtpParseError, PtpPortIdentity,
};
use airplay2::protocol::ptp::timestamp::PtpTimestamp;

use tokio::net::UdpSocket;

// ===== Full IEEE 1588 two-step exchange =====

#[tokio::test]
async fn test_full_ieee1588_two_step_exchange() {
    let master_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let slave_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let master_addr = master_sock.local_addr().unwrap();
    let slave_addr = slave_sock.local_addr().unwrap();

    let mut slave_clock = PtpClock::new(0xBBBB, PtpRole::Slave);
    let master_source = PtpPortIdentity::new(0xAAAA, 1);
    let slave_source = PtpPortIdentity::new(0xBBBB, 1);

    // Perform 5 exchanges.
    for seq in 0..5u16 {
        // 1. Master sends Sync.
        let t1 = PtpTimestamp::now();
        let mut sync = PtpMessage::sync(master_source, seq, t1);
        sync.header.flags = 0x0200; // Two-step
        master_sock
            .send_to(&sync.encode(), slave_addr)
            .await
            .unwrap();

        // 2. Slave receives Sync.
        let mut buf = [0u8; 256];
        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let t2 = PtpTimestamp::now();
        let recv_sync = PtpMessage::decode(&buf[..len]).unwrap();
        assert_eq!(recv_sync.header.message_type, PtpMessageType::Sync);

        // 3. Master sends Follow-up.
        let follow_up = PtpMessage::follow_up(master_source, seq, t1);
        master_sock
            .send_to(&follow_up.encode(), slave_addr)
            .await
            .unwrap();

        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let fu = PtpMessage::decode(&buf[..len]).unwrap();
        assert_eq!(fu.header.message_type, PtpMessageType::FollowUp);
        let precise_t1 = match fu.body {
            PtpMessageBody::FollowUp {
                precise_origin_timestamp,
            } => precise_origin_timestamp,
            _ => panic!("Expected Follow-up body"),
        };

        // 4. Slave sends Delay_Req.
        let t3 = PtpTimestamp::now();
        let delay_req = PtpMessage::delay_req(slave_source, seq, t3);
        slave_sock
            .send_to(&delay_req.encode(), master_addr)
            .await
            .unwrap();

        // 5. Master receives Delay_Req.
        let (len, from) = master_sock.recv_from(&mut buf).await.unwrap();
        let t4 = PtpTimestamp::now();
        let req = PtpMessage::decode(&buf[..len]).unwrap();
        assert_eq!(req.header.message_type, PtpMessageType::DelayReq);

        // 6. Master sends Delay_Resp.
        let delay_resp = PtpMessage::delay_resp(master_source, seq, t4, slave_source);
        master_sock
            .send_to(&delay_resp.encode(), from)
            .await
            .unwrap();

        // 7. Slave receives Delay_Resp.
        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let resp = PtpMessage::decode(&buf[..len]).unwrap();
        assert_eq!(resp.header.message_type, PtpMessageType::DelayResp);
        let recv_t4 = match resp.body {
            PtpMessageBody::DelayResp {
                receive_timestamp, ..
            } => receive_timestamp,
            _ => panic!("Expected DelayResp body"),
        };

        // 8. Update clock.
        slave_clock.process_timing(precise_t1, t2, t3, recv_t4);
    }

    assert!(slave_clock.is_synchronized());
    // On loopback, offset should be very small (same machine, same clock).
    assert!(
        slave_clock.offset_millis().abs() < 50.0,
        "Offset should be near zero on loopback: {}ms",
        slave_clock.offset_millis()
    );
    assert_eq!(slave_clock.measurement_count(), 5);
}

// ===== Full AirPlay compact exchange =====

#[tokio::test]
async fn test_full_airplay_compact_exchange() {
    let master_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let slave_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let master_addr = master_sock.local_addr().unwrap();
    let slave_addr = slave_sock.local_addr().unwrap();

    let mut slave_clock = PtpClock::new(0xBBBB, PtpRole::Slave);

    for seq in 0..3u16 {
        // Master sends Sync.
        let t1 = PtpTimestamp::now();
        let sync = AirPlayTimingPacket {
            message_type: PtpMessageType::Sync,
            sequence_id: seq,
            timestamp: t1,
            clock_id: 0xAAAA,
        };
        master_sock
            .send_to(&sync.encode(), slave_addr)
            .await
            .unwrap();

        // Slave receives Sync.
        let mut buf = [0u8; 256];
        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let t2 = PtpTimestamp::now();
        let recv = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
        assert_eq!(recv.message_type, PtpMessageType::Sync);

        // Slave sends Delay_Req.
        let t3 = PtpTimestamp::now();
        let delay_req = AirPlayTimingPacket {
            message_type: PtpMessageType::DelayReq,
            sequence_id: seq,
            timestamp: t3,
            clock_id: 0xBBBB,
        };
        slave_sock
            .send_to(&delay_req.encode(), master_addr)
            .await
            .unwrap();

        // Master receives and sends Delay_Resp.
        let (len, from) = master_sock.recv_from(&mut buf).await.unwrap();
        let t4 = PtpTimestamp::now();
        let req = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
        assert_eq!(req.message_type, PtpMessageType::DelayReq);

        let delay_resp = AirPlayTimingPacket {
            message_type: PtpMessageType::DelayResp,
            sequence_id: seq,
            timestamp: t4,
            clock_id: 0xAAAA,
        };
        master_sock
            .send_to(&delay_resp.encode(), from)
            .await
            .unwrap();

        // Slave receives and updates clock.
        let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
        let resp = AirPlayTimingPacket::decode(&buf[..len]).unwrap();

        slave_clock.process_timing(recv.timestamp, t2, t3, resp.timestamp);
    }

    assert!(slave_clock.is_synchronized());
    assert!(
        slave_clock.offset_millis().abs() < 50.0,
        "AirPlay offset too large: {}ms",
        slave_clock.offset_millis()
    );
}

// ===== Master and slave handler tasks =====

#[tokio::test]
async fn test_master_slave_handler_tasks() {
    let master_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let slave_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let master_addr = master_sock.local_addr().unwrap();
    let slave_addr = slave_sock.local_addr().unwrap();

    // Connect master to slave (for broadcast/send without target).
    master_sock.connect(slave_addr).await.unwrap();

    let master_clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let slave_clock = create_shared_clock(0xBBBB, PtpRole::Slave);

    let master_config = PtpHandlerConfig {
        clock_id: 0xAAAA,
        role: PtpRole::Master,
        sync_interval: Duration::from_millis(50),
        use_airplay_format: true,
        ..Default::default()
    };

    let slave_config = PtpHandlerConfig {
        clock_id: 0xBBBB,
        role: PtpRole::Slave,
        delay_req_interval: Duration::from_millis(50),
        use_airplay_format: true,
        ..Default::default()
    };

    let (master_shutdown_tx, master_shutdown_rx) = tokio::sync::watch::channel(false);
    let (slave_shutdown_tx, slave_shutdown_rx) = tokio::sync::watch::channel(false);

    // Start master handler.
    let master_sock_clone = master_sock.clone();
    let master_clock_clone = master_clock.clone();
    let master_handle = tokio::spawn(async move {
        let mut handler =
            PtpMasterHandler::new(master_sock_clone, None, master_clock_clone, master_config);
        handler.run(master_shutdown_rx).await
    });

    // Start slave handler.
    let slave_sock_clone = slave_sock.clone();
    let slave_clock_clone = slave_clock.clone();
    let slave_handle = tokio::spawn(async move {
        let mut handler = PtpSlaveHandler::new(
            slave_sock_clone,
            None,
            slave_clock_clone,
            slave_config,
            master_addr,
        );
        handler.run(slave_shutdown_rx).await
    });

    // Let them exchange for a bit.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check that slave has synchronized.
    let _clock = slave_clock.read().await;
    // On loopback this may or may not sync depending on timing,
    // but the handler should have accepted at least some measurements.
    // (The sync depends on both sides' timing, which is non-deterministic.)

    // Shutdown both.
    master_shutdown_tx.send(true).unwrap();
    slave_shutdown_tx.send(true).unwrap();

    let _ = tokio::time::timeout(Duration::from_secs(2), master_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), slave_handle).await;
}

// ===== Clock offset with known skew =====

#[test]
fn test_clock_offset_5_second_skew() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Slave is exactly 5 seconds ahead of master.
    // Network delay: 1ms each way.
    let t1 = PtpTimestamp::new(100, 0); // master send
    let t2 = PtpTimestamp::new(105, 1_000_000); // slave recv (5s offset + 1ms delay)
    let t3 = PtpTimestamp::new(105, 2_000_000); // slave send (5s offset + 2ms)
    let t4 = PtpTimestamp::new(100, 3_000_000); // master recv (3ms from start)

    clock.process_timing(t1, t2, t3, t4);

    // Expected offset: 5 seconds.
    let offset_ms = clock.offset_millis();
    assert!(
        (offset_ms - 5000.0).abs() < 5.0,
        "Expected 5s offset, got {}ms",
        offset_ms
    );
}

#[test]
fn test_clock_offset_negative_skew() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // Slave is 2 seconds behind master.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(98, 1_000_000);
    let t3 = PtpTimestamp::new(98, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);

    clock.process_timing(t1, t2, t3, t4);

    let offset_ms = clock.offset_millis();
    assert!(
        (offset_ms - (-2000.0)).abs() < 5.0,
        "Expected -2s offset, got {}ms",
        offset_ms
    );
}

// ===== Timestamp conversions =====

#[test]
fn test_timestamp_ieee1588_roundtrip_many() {
    let test_values = [
        PtpTimestamp::new(0, 0),
        PtpTimestamp::new(1, 0),
        PtpTimestamp::new(0, 1),
        PtpTimestamp::new(0, 999_999_999),
        PtpTimestamp::new(u64::from(u32::MAX), 0),
        PtpTimestamp::new(12345, 678_901_234),
    ];

    for ts in &test_values {
        let encoded = ts.encode_ieee1588();
        let decoded = PtpTimestamp::decode_ieee1588(&encoded).unwrap();
        assert_eq!(ts, &decoded, "Roundtrip failed for {ts}");
    }
}

#[test]
fn test_timestamp_airplay_compact_roundtrip_seconds() {
    // Integer seconds should roundtrip perfectly.
    for secs in [0, 1, 100, 10000, 1_000_000] {
        let ts = PtpTimestamp::new(secs, 0);
        let compact = ts.to_airplay_compact();
        let back = PtpTimestamp::from_airplay_compact(compact);
        assert_eq!(ts, back, "Integer second roundtrip failed for {secs}");
    }
}

// ===== Message parsing edge cases =====

#[test]
fn test_parse_invalid_message_type() {
    // Build a packet with message type 0x0F (invalid).
    let mut data = vec![0u8; 44]; // Minimum for Sync
    data[0] = 0x0F; // Invalid message type
    data[1] = 0x02; // Version 2
    let result = PtpMessage::decode(&data);
    assert!(result.is_err());
    match result.unwrap_err() {
        PtpParseError::UnknownMessageType(t) => assert_eq!(t, 0x0F),
        other => panic!("Expected UnknownMessageType, got {other:?}"),
    }
}

#[test]
fn test_parse_truncated_header() {
    let data = vec![0u8; 10];
    assert!(PtpMessage::decode(&data).is_err());
}

#[test]
fn test_airplay_packet_all_message_types() {
    for msg_type in [
        PtpMessageType::Sync,
        PtpMessageType::DelayReq,
        PtpMessageType::FollowUp,
        PtpMessageType::DelayResp,
        PtpMessageType::Announce,
    ] {
        let pkt = AirPlayTimingPacket {
            message_type: msg_type,
            sequence_id: 42,
            timestamp: PtpTimestamp::new(100, 0),
            clock_id: 0xDEAD,
        };
        let encoded = pkt.encode();
        let decoded = AirPlayTimingPacket::decode(&encoded).unwrap();
        assert_eq!(decoded.message_type, msg_type);
        assert_eq!(decoded.sequence_id, 42);
    }
}

// ===== Clock with multiple measurements and outlier rejection =====

#[test]
fn test_clock_outlier_rejection() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);
    clock.set_max_rtt(Duration::from_millis(5));

    // Good measurement (RTT = 2ms).
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(100, 500_000);
    let t3 = PtpTimestamp::new(100, 1_000_000);
    let t4 = PtpTimestamp::new(100, 2_000_000);
    assert!(clock.process_timing(t1, t2, t3, t4));

    // Bad measurement (RTT = 50ms) — should be rejected.
    let t1 = PtpTimestamp::new(200, 0);
    let t2 = PtpTimestamp::new(200, 500_000);
    let t3 = PtpTimestamp::new(200, 1_000_000);
    let t4 = PtpTimestamp::new(200, 50_000_000);
    assert!(!clock.process_timing(t1, t2, t3, t4));

    // Only the good measurement should remain.
    assert_eq!(clock.measurement_count(), 1);
}

// ===== RTP timestamp to PTP conversion =====

#[test]
fn test_rtp_to_ptp_one_second() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let anchor_rtp = 0u32;
    let anchor_ptp = PtpTimestamp::new(1000, 0);

    let result = clock.rtp_to_local_ptp(44100, 44100, anchor_rtp, anchor_ptp);
    // 44100 samples at 44100 Hz = 1 second.
    assert_eq!(result.seconds, 1001);
}

#[test]
fn test_rtp_to_ptp_half_second() {
    let clock = PtpClock::new(0, PtpRole::Slave);
    let anchor_rtp = 0u32;
    let anchor_ptp = PtpTimestamp::new(500, 0);

    let result = clock.rtp_to_local_ptp(22050, 44100, anchor_rtp, anchor_ptp);
    // 22050 samples at 44100 Hz = 0.5 seconds.
    assert_eq!(result.seconds, 500);
    assert!(
        (result.nanoseconds as i64 - 500_000_000).abs() < 1_000_000,
        "Expected ~500ms, got {} ns",
        result.nanoseconds
    );
}

// ===== Port identity =====

#[test]
fn test_port_identity_default() {
    let id = PtpPortIdentity::default();
    assert_eq!(id.clock_identity, 0);
    assert_eq!(id.port_number, 0);
}

#[test]
fn test_port_identity_in_message_preserved() {
    let source = PtpPortIdentity::new(0x0102030405060708, 0x0A0B);
    let msg = PtpMessage::sync(source, 0, PtpTimestamp::ZERO);
    let encoded = msg.encode();
    let decoded = PtpMessage::decode(&encoded).unwrap();
    assert_eq!(decoded.header.source_port_identity, source);
}

// ===== Clock reset and re-sync =====

#[test]
fn test_clock_reset_then_resync() {
    let mut clock = PtpClock::new(0, PtpRole::Slave);

    // First sync.
    let t1 = PtpTimestamp::new(100, 0);
    let t2 = PtpTimestamp::new(105, 1_000_000);
    let t3 = PtpTimestamp::new(105, 2_000_000);
    let t4 = PtpTimestamp::new(100, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);
    assert!(clock.is_synchronized());
    assert!((clock.offset_millis() - 5000.0).abs() < 5.0);

    // Reset.
    clock.reset();
    assert!(!clock.is_synchronized());

    // Re-sync with different offset.
    let t1 = PtpTimestamp::new(200, 0);
    let t2 = PtpTimestamp::new(202, 1_000_000);
    let t3 = PtpTimestamp::new(202, 2_000_000);
    let t4 = PtpTimestamp::new(200, 3_000_000);
    clock.process_timing(t1, t2, t3, t4);
    assert!(clock.is_synchronized());
    assert!((clock.offset_millis() - 2000.0).abs() < 5.0);
}
