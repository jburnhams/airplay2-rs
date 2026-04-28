import re

with open('tests/ptp_integration.rs', 'r') as f:
    content = f.read()

# Replace the loop with what was essentially done in the other test (test_immediate_delay_req_on_follow_up)
replacement = '''    let mut msg_type = None;
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

    let _ = shutdown_tx.send(true);
    let _ = node_handle.await;

    assert_eq!(
        msg_type,
        Some(PtpMessageType::DelayReq),
        "Second Delay_Req must be sent after new Sync resets pending_t3"
    );'''

content = re.sub(r'    let len;\n    loop \{.*?    \);\n}', replacement + '\n}', content, flags=re.DOTALL)

with open('tests/ptp_integration.rs', 'w') as f:
    f.write(content)
