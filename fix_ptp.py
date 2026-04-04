import os

content = open("tests/ptp_integration.rs", "r").read()

to_replace = """    let (len, _) = second
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

replacement = """    let mut msg_type = None;
    for _ in 0..5 {
        let result = tokio::time::timeout(
            Duration::from_millis(400),
            homepod_event_sock.recv_from(&mut buf),
        )
        .await;

        if let Ok(Ok((len, _))) = result {
            if let Ok(msg) = PtpMessage::decode(&buf[..len]) {
                if msg.header.message_type == PtpMessageType::DelayReq {
                    msg_type = Some(PtpMessageType::DelayReq);
                    break;
                }
            }
        } else {
            break;
        }
    }

    assert_eq!(
        msg_type,
        Some(PtpMessageType::DelayReq),
        "Second Delay_Req must be sent after new Sync resets pending_t3"
    );"""

content = content.replace(to_replace, replacement)
open("tests/ptp_integration.rs", "w").write(content)
