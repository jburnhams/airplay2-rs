use crate::testing::packet_capture::*;

#[test]
fn test_capture_replay() {
    let packets = vec![
        CapturedPacket {
            timestamp_us: 0,
            inbound: true,
            protocol: CaptureProtocol::Tcp,
            data: vec![1, 2, 3],
        },
        CapturedPacket {
            timestamp_us: 1000,
            inbound: false,
            protocol: CaptureProtocol::Tcp,
            data: vec![4, 5, 6],
        },
        CapturedPacket {
            timestamp_us: 2000,
            inbound: true,
            protocol: CaptureProtocol::Tcp,
            data: vec![7, 8, 9],
        },
    ];

    let mut replay = CaptureReplay::new(packets);

    // Should get inbound packets only
    let p1 = replay.next_inbound().unwrap();
    assert_eq!(p1.data, vec![1, 2, 3]);

    let p2 = replay.next_inbound().unwrap();
    assert_eq!(p2.data, vec![7, 8, 9]);

    assert!(replay.next_inbound().is_none());
}
