#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use airplay2::protocol::pairing::tlv::{TlvEncoder, TlvType};

    #[test]
    fn generate_capture_fixtures() {
        let captures_dir = Path::new("tests/captures");
        if !captures_dir.exists() {
            fs::create_dir_all(captures_dir).unwrap();
        }

        // 1. Info Request Capture
        let info_request = b"GET /info RTSP/1.0\r\n\
                             CSeq: 1\r\n\
                             User-Agent: AirPlay/609.10.11\r\n\
                             X-Apple-Device-ID: 0x112233445566\r\n\
                             X-Apple-Client-Name: My iPhone\r\n\
                             \r\n";

        let mut info_hex = String::new();
        info_hex.push_str("# Captured /info request\n");
        info_hex.push_str(&format!("0 IN TCP {}\n", hex::encode(info_request)));

        fs::write(captures_dir.join("info_request.hex"), info_hex)
            .expect("Failed to write info_request.hex");

        // 2. Pairing Exchange Capture
        let m1_body = TlvEncoder::new()
            .add_state(1)
            .add_byte(TlvType::Method, 0)
            .build();

        let mut pair_request = format!(
            "POST /pair-setup RTSP/1.0\r\nCSeq: 2\r\nContent-Length: {}\r\nContent-Type: \
             application/octet-stream\r\n\r\n",
            m1_body.len()
        )
        .into_bytes();
        pair_request.extend_from_slice(&m1_body);

        let mut pairing_hex = String::new();
        pairing_hex.push_str("# Captured pairing exchange\n");
        pairing_hex.push_str(&format!("0 IN TCP {}\n", hex::encode(pair_request)));

        fs::write(captures_dir.join("pairing_exchange.hex"), pairing_hex)
            .expect("Failed to write pairing_exchange.hex");

        println!("Captures generated in tests/captures/");
    }
}
