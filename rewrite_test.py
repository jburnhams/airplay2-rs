import re

with open('tests/ptp_integration.rs', 'r') as f:
    content = f.read()

# Replace the block checking `second` with a loop
# Find from: `let second = tokio::time::timeout(` to the end of the function

pattern = r'    let second = tokio::time::timeout\([\s\S]*?Second Delay_Req must be sent after new Sync resets pending_t3"\n    \);'

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

    let _ = shutdown_tx.send(true);
    let _ = node_handle.await;

    assert!(received_delay_req, "Second Delay_Req must be sent after new Sync resets pending_t3");"""

if re.search(pattern, content):
    content = re.sub(pattern, replacement, content)
    with open('tests/ptp_integration.rs', 'w') as f:
        f.write(content)
    print("Replaced successfully")
else:
    print("Pattern not found")
