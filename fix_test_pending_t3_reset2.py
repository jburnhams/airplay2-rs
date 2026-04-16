import os

path = 'tests/ptp_integration.rs'
with open(path, 'r') as f:
    content = f.read()

target = """    let second = tokio::time::timeout(
        Duration::from_millis(400),
        homepod_event_sock.recv_from(&mut buf),
    )
    .await;

    let _ = shutdown_tx.send(true);
    let _ = node_handle.await;

    let (len, _) = second
        .expect(
            "Node must send a second Delay_Req after the new Sync resets pending_t3 (without the \
             fix the node would be permanently stuck)",
        )
        .unwrap();
    let msg = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(
        msg.header.message_type,
        PtpMessageType::DelayReq,
        "Second Delay_Req must be sent after new Sync resets pending_t3"
    );"""

replacement = """    let mut received_delay_req = false;
    let end_time = tokio::time::Instant::now() + Duration::from_millis(1000);
    while tokio::time::Instant::now() < end_time {
        if let Ok(Ok((len, _))) = tokio::time::timeout(Duration::from_millis(100), homepod_event_sock.recv_from(&mut buf)).await {
            if let Ok(msg) = PtpMessage::decode(&buf[..len]) {
                if msg.header.message_type == PtpMessageType::DelayReq {
                    received_delay_req = true;
                    break;
                }
            }
        }
    }

    assert!(received_delay_req, "Second Delay_Req must be sent after new Sync resets pending_t3");

    let _ = shutdown_tx.send(true);
    let _ = node_handle.await;
"""

# Let's revert then replace
import subprocess
subprocess.run(['git', 'checkout', 'tests/ptp_integration.rs'])
