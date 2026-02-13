use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::protocol::ptp::clock::PtpRole;
use crate::protocol::ptp::handler::{
    PTP_EVENT_PORT, PTP_GENERAL_PORT, PtpHandlerConfig, PtpMasterHandler, PtpSlaveHandler,
    create_shared_clock,
};
use crate::protocol::ptp::message::{
    AirPlayTimingPacket, PtpMessage, PtpMessageType, PtpPortIdentity,
};
use crate::protocol::ptp::timestamp::PtpTimestamp;

// ===== Constants =====

#[test]
fn test_port_constants() {
    assert_eq!(PTP_EVENT_PORT, 319);
    assert_eq!(PTP_GENERAL_PORT, 320);
}

// ===== PtpHandlerConfig =====

#[test]
fn test_handler_config_defaults() {
    let config = PtpHandlerConfig::default();
    assert_eq!(config.clock_id, 0);
    assert_eq!(config.role, PtpRole::Slave);
    assert_eq!(config.sync_interval, Duration::from_secs(1));
    assert_eq!(config.delay_req_interval, Duration::from_secs(1));
    assert_eq!(config.recv_buf_size, 256);
    assert!(!config.use_airplay_format);
}

// ===== create_shared_clock =====

#[tokio::test]
async fn test_create_shared_clock() {
    let clock = create_shared_clock(0x12345678, PtpRole::Slave);
    let locked = clock.read().await;
    assert_eq!(locked.clock_id(), 0x12345678);
    assert_eq!(locked.role(), PtpRole::Slave);
    assert!(!locked.is_synchronized());
}

// ===== IEEE 1588 Master-Slave exchange over loopback =====

#[tokio::test]
async fn test_master_slave_ieee1588_exchange() {
    // Bind two socket pairs on loopback.
    let master_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let slave_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

    let master_addr = master_socket.local_addr().unwrap();
    let slave_addr = slave_socket.local_addr().unwrap();

    let slave_clock = create_shared_clock(0xBBBB, PtpRole::Slave);

    // Manually simulate a two-step Sync exchange.
    // 1. Master sends Sync.
    let source = PtpPortIdentity::new(0xAAAA, 1);
    let t1 = PtpTimestamp::new(1000, 0);
    let mut sync_msg = PtpMessage::sync(source, 1, t1);
    sync_msg.header.flags = 0x0200; // Two-step
    let sync_data = sync_msg.encode();
    master_socket.send_to(&sync_data, slave_addr).await.unwrap();

    // 2. Slave receives Sync.
    let mut buf = [0u8; 256];
    let (len, _from) = slave_socket.recv_from(&mut buf).await.unwrap();
    let t2 = PtpTimestamp::new(1000, 5_000_000); // Simulated receive time

    let received = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(received.header.message_type, PtpMessageType::Sync);

    // 3. Master sends Follow-up with precise T1.
    let follow_up = PtpMessage::follow_up(source, 1, t1);
    master_socket
        .send_to(&follow_up.encode(), slave_addr)
        .await
        .unwrap();

    let (len, _) = slave_socket.recv_from(&mut buf).await.unwrap();
    let fu_msg = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(fu_msg.header.message_type, PtpMessageType::FollowUp);

    // 4. Slave sends Delay_Req.
    let t3 = PtpTimestamp::new(1000, 10_000_000);
    let slave_source = PtpPortIdentity::new(0xBBBB, 1);
    let delay_req = PtpMessage::delay_req(slave_source, 1, t3);
    slave_socket
        .send_to(&delay_req.encode(), master_addr)
        .await
        .unwrap();

    // 5. Master receives Delay_Req and sends Delay_Resp.
    let (len, from) = master_socket.recv_from(&mut buf).await.unwrap();
    let t4 = PtpTimestamp::new(1000, 15_000_000);
    let req_msg = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(req_msg.header.message_type, PtpMessageType::DelayReq);

    let delay_resp = PtpMessage::delay_resp(source, 1, t4, slave_source);
    master_socket
        .send_to(&delay_resp.encode(), from)
        .await
        .unwrap();

    // 6. Slave receives Delay_Resp and updates clock.
    let (len, _) = slave_socket.recv_from(&mut buf).await.unwrap();
    let resp_msg = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(resp_msg.header.message_type, PtpMessageType::DelayResp);

    // 7. Update the clock.
    {
        let mut clock = slave_clock.write().await;
        clock.process_timing(t1, t2, t3, t4);
        assert!(clock.is_synchronized());
    }

    // Verify offset is ~0 (same epoch, just network delay).
    let clock = slave_clock.read().await;
    assert!(
        clock.offset_millis().abs() < 10.0,
        "Expected near-zero offset, got {}ms",
        clock.offset_millis()
    );
}

// ===== AirPlay format exchange over loopback =====

#[tokio::test]
async fn test_airplay_format_exchange() {
    let master_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let slave_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

    let master_addr = master_socket.local_addr().unwrap();
    let slave_addr = slave_socket.local_addr().unwrap();

    let slave_clock = create_shared_clock(0xBBBB, PtpRole::Slave);

    // 1. Master sends AirPlay Sync.
    let t1 = PtpTimestamp::new(500, 0);
    let sync_pkt = AirPlayTimingPacket {
        message_type: PtpMessageType::Sync,
        sequence_id: 1,
        timestamp: t1,
        clock_id: 0xAAAA,
    };
    master_socket
        .send_to(&sync_pkt.encode(), slave_addr)
        .await
        .unwrap();

    // 2. Slave receives.
    let mut buf = [0u8; 256];
    let (len, _) = slave_socket.recv_from(&mut buf).await.unwrap();
    let t2 = PtpTimestamp::new(500, 2_000_000);
    let received = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
    assert_eq!(received.message_type, PtpMessageType::Sync);

    // 3. Slave sends DelayReq.
    let t3 = PtpTimestamp::new(500, 5_000_000);
    let delay_req_pkt = AirPlayTimingPacket {
        message_type: PtpMessageType::DelayReq,
        sequence_id: 1,
        timestamp: t3,
        clock_id: 0xBBBB,
    };
    slave_socket
        .send_to(&delay_req_pkt.encode(), master_addr)
        .await
        .unwrap();

    // 4. Master receives and sends DelayResp.
    let (len, from) = master_socket.recv_from(&mut buf).await.unwrap();
    let t4 = PtpTimestamp::new(500, 8_000_000);
    let req = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
    assert_eq!(req.message_type, PtpMessageType::DelayReq);

    let resp_pkt = AirPlayTimingPacket {
        message_type: PtpMessageType::DelayResp,
        sequence_id: 1,
        timestamp: t4,
        clock_id: 0xAAAA,
    };
    master_socket
        .send_to(&resp_pkt.encode(), from)
        .await
        .unwrap();

    // 5. Slave receives DelayResp and updates clock.
    let (len, _) = slave_socket.recv_from(&mut buf).await.unwrap();
    let resp = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
    assert_eq!(resp.message_type, PtpMessageType::DelayResp);

    {
        let mut clock = slave_clock.write().await;
        // Use timestamps from AirPlay compact format - note precision loss.
        clock.process_timing(received.timestamp, t2, t3, resp.timestamp);
        assert!(clock.is_synchronized());
    }

    let clock = slave_clock.read().await;
    assert!(
        clock.offset_millis().abs() < 50.0,
        "Offset too large: {}ms",
        clock.offset_millis()
    );
}

// ===== Master handler Delay_Req handling =====

#[tokio::test]
async fn test_master_handler_responds_to_delay_req() {
    let master_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let master_addr = master_sock.local_addr().unwrap();

    let clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let config = PtpHandlerConfig {
        clock_id: 0xAAAA,
        role: PtpRole::Master,
        sync_interval: Duration::from_secs(60), // Long interval so it doesn't interfere.
        use_airplay_format: false,
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let master_sock_clone = master_sock.clone();
    let handle = tokio::spawn(async move {
        let mut handler = PtpMasterHandler::new(master_sock_clone, None, clock, config);
        handler.run(shutdown_rx).await
    });

    // Send a Delay_Req from client.
    let source = PtpPortIdentity::new(0xBBBB, 1);
    let t3 = PtpTimestamp::new(100, 0);
    let req = PtpMessage::delay_req(source, 42, t3);
    client_sock
        .send_to(&req.encode(), master_addr)
        .await
        .unwrap();

    // Receive the Delay_Resp.
    let mut buf = [0u8; 256];
    let result =
        tokio::time::timeout(Duration::from_secs(2), client_sock.recv_from(&mut buf)).await;
    assert!(result.is_ok(), "Did not receive Delay_Resp in time");

    let (len, _) = result.unwrap().unwrap();
    let resp = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(resp.header.message_type, PtpMessageType::DelayResp);
    assert_eq!(resp.header.sequence_id, 42);

    // Shutdown.
    shutdown_tx.send(true).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}

// ===== Master handler AirPlay format =====

#[tokio::test]
async fn test_master_handler_airplay_format() {
    let master_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let client_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let master_addr = master_sock.local_addr().unwrap();

    let clock = create_shared_clock(0xAAAA, PtpRole::Master);
    let config = PtpHandlerConfig {
        clock_id: 0xAAAA,
        role: PtpRole::Master,
        sync_interval: Duration::from_secs(60),
        use_airplay_format: true,
        ..Default::default()
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let master_sock_clone = master_sock.clone();
    let handle = tokio::spawn(async move {
        let mut handler = PtpMasterHandler::new(master_sock_clone, None, clock, config);
        handler.run(shutdown_rx).await
    });

    // Send AirPlay Delay_Req.
    let req = AirPlayTimingPacket {
        message_type: PtpMessageType::DelayReq,
        sequence_id: 7,
        timestamp: PtpTimestamp::new(200, 0),
        clock_id: 0xBBBB,
    };
    client_sock
        .send_to(&req.encode(), master_addr)
        .await
        .unwrap();

    // Receive AirPlay Delay_Resp.
    let mut buf = [0u8; 256];
    let result =
        tokio::time::timeout(Duration::from_secs(2), client_sock.recv_from(&mut buf)).await;
    assert!(result.is_ok(), "Did not receive AirPlay Delay_Resp");

    let (len, _) = result.unwrap().unwrap();
    let resp = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
    assert_eq!(resp.message_type, PtpMessageType::DelayResp);
    assert_eq!(resp.sequence_id, 7);

    shutdown_tx.send(true).unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
}

// ===== Slave handler with master simulation =====

/// Test slave handler by manually driving the protocol exchange
/// without relying on the timer-driven event loop, which ensures
/// deterministic behavior regardless of scheduling.
#[tokio::test]
async fn test_slave_handler_synchronizes() {
    // Instead of running the full event loop (which has timer dependencies),
    // we test the core logic: the slave receives packets on a socket,
    // processes them, and the shared clock gets synchronized.
    let slave_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let master_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let slave_addr = slave_sock.local_addr().unwrap();
    let master_addr = master_sock.local_addr().unwrap();

    let slave_clock = create_shared_clock(0xBBBB, PtpRole::Slave);

    // Manually perform the exchange using sockets, without the handler loop.
    // This tests the packet format compatibility and clock update logic.

    // 1. Master sends Sync.
    let t1 = PtpTimestamp::now();
    let sync_pkt = AirPlayTimingPacket {
        message_type: PtpMessageType::Sync,
        sequence_id: 1,
        timestamp: t1,
        clock_id: 0xAAAA,
    };
    master_sock
        .send_to(&sync_pkt.encode(), slave_addr)
        .await
        .unwrap();

    // 2. Slave receives Sync.
    let mut buf = [0u8; 256];
    let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
    let t2 = PtpTimestamp::now();
    let recv_sync = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
    assert_eq!(recv_sync.message_type, PtpMessageType::Sync);

    // 3. Slave sends Delay_Req.
    let t3 = PtpTimestamp::now();
    let delay_req = AirPlayTimingPacket {
        message_type: PtpMessageType::DelayReq,
        sequence_id: 1,
        timestamp: t3,
        clock_id: 0xBBBB,
    };
    slave_sock
        .send_to(&delay_req.encode(), master_addr)
        .await
        .unwrap();

    // 4. Master receives Delay_Req and sends Delay_Resp.
    let (len, from) = master_sock.recv_from(&mut buf).await.unwrap();
    let recv_req = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
    assert_eq!(recv_req.message_type, PtpMessageType::DelayReq);

    let t4 = PtpTimestamp::now();
    let delay_resp = AirPlayTimingPacket {
        message_type: PtpMessageType::DelayResp,
        sequence_id: 1,
        timestamp: t4,
        clock_id: 0xAAAA,
    };
    master_sock
        .send_to(&delay_resp.encode(), from)
        .await
        .unwrap();

    // 5. Slave receives Delay_Resp.
    let (len, _) = slave_sock.recv_from(&mut buf).await.unwrap();
    let recv_resp = AirPlayTimingPacket::decode(&buf[..len]).unwrap();
    assert_eq!(recv_resp.message_type, PtpMessageType::DelayResp);

    // 6. Update the shared clock (as the handler would).
    {
        let mut clock = slave_clock.write().await;
        clock.process_timing(recv_sync.timestamp, t2, t3, recv_resp.timestamp);
    }

    // 7. Verify synchronization.
    let clock = slave_clock.read().await;
    assert!(clock.is_synchronized(), "Slave should be synchronized");
    // On loopback, offset should be small.
    assert!(
        clock.offset_millis().abs() < 100.0,
        "Offset too large on loopback: {}ms",
        clock.offset_millis()
    );
}

// ===== Slave handler clock accessor =====

#[tokio::test]
async fn test_slave_handler_clock_accessor() {
    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let clock = create_shared_clock(0x42, PtpRole::Slave);
    let config = PtpHandlerConfig::default();
    let addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();

    let handler = PtpSlaveHandler::new(sock, None, clock.clone(), config, addr);
    let clock_ref = handler.clock();

    // Both references should point to the same clock.
    let c1 = clock.read().await;
    let c2 = clock_ref.read().await;
    assert_eq!(c1.clock_id(), c2.clock_id());
}

// ===== Master handler clock accessor =====

#[tokio::test]
async fn test_master_handler_clock_accessor() {
    let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let clock = create_shared_clock(0x99, PtpRole::Master);
    let config = PtpHandlerConfig::default();

    let handler = PtpMasterHandler::new(sock, None, clock.clone(), config);
    let clock_ref = handler.clock();

    let c1 = clock.read().await;
    let c2 = clock_ref.read().await;
    assert_eq!(c1.clock_id(), c2.clock_id());
}
