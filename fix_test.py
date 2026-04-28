import re

with open('tests/ptp_integration.rs', 'r') as f:
    content = f.read()

content = content.replace('''    let second = tokio::time::timeout(
        Duration::from_millis(400),
        homepod_event_sock.recv_from(&mut buf),
    )
    .await;

    let _ = shutdown_tx.send(true);
    let _ = node_handle.await;

    let (len, _) = second
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
    );''', '''    let mut len = 0;
    loop {
        let second = tokio::time::timeout(
            Duration::from_millis(400),
            homepod_event_sock.recv_from(&mut buf),
        )
        .await;

        let (l, _) = second
            .expect(
                "Node must send a second Delay_Req after the new Sync resets pending_t3 (without the \\
                 fix the node would be permanently stuck)",
            )
            .unwrap();
        let msg = PtpMessage::decode(&buf[..l]).unwrap();
        if msg.header.message_type == PtpMessageType::DelayReq {
            len = l;
            break;
        }
    }
    let _ = shutdown_tx.send(true);
    let _ = node_handle.await;

    let msg = PtpMessage::decode(&buf[..len]).unwrap();
    assert_eq!(
        msg.header.message_type,
        PtpMessageType::DelayReq,
        "Second Delay_Req must be sent after new Sync resets pending_t3"
    );''')

with open('tests/ptp_integration.rs', 'w') as f:
    f.write(content)
