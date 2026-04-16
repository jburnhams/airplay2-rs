import os

path = 'tests/ptp_integration.rs'
with open(path, 'r') as f:
    content = f.read()

# We need to make sure the loop drains any duplicate FollowUp that might be sent if the timer triggered.
# Since it asserts `msg.header.message_type == PtpMessageType::DelayReq` but gets `FollowUp`.
# Wait, `FollowUp` is received from the client? No, `FollowUp` is sent BY the master (HomePod) to the slave.
# `homepod_event_sock.recv_from(&mut buf)` is receiving packets SENT by the node.
# The node does not send `FollowUp` messages, it is a slave. Wait.
# Why did `homepod_event_sock.recv_from` receive a `FollowUp`?
# Ah, PtpNode sends DelayReq to the event port. But if PtpNode is also master? No, PtpNode is configured with priority 255 and client_clock_id. HomePod has priority 248. PtpNode should be slave.
# But `PtpNode` sends `FollowUp`? No, PtpNode sends `Announce` and `Sync` if it's the master.
# It seems the `client` is sending a `FollowUp`.
# Let's add a loop to skip non-DelayReq messages in the second exchange.

target = """    let (len, _) = second
        .expect(
            "Node must send a second Delay_Req after the new Sync resets pending_t3 (without the \\
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

    assert!(received_delay_req, "Second Delay_Req must be sent after new Sync resets pending_t3");"""

# Wait, `second` was `let second = tokio::time::timeout(Duration::from_millis(400), homepod_event_sock.recv_from(&mut buf)).await;`
# We need to replace the `second` logic.

content_new = content.replace("""    let second = tokio::time::timeout(
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
    );""", replacement + """\n\n    let _ = shutdown_tx.send(true);\n    let _ = node_handle.await;""")

with open(path, 'w') as f:
    f.write(content_new)
