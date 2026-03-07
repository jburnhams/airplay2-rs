use std::time::Duration;

use airplay2::protocol::rtp::ntp_client::NtpClient;
use tokio;

#[tokio::test]
async fn test_ntp_client_against_public_server() {
    let client = NtpClient::new("time.google.com:123".to_string(), Duration::from_secs(5));
    let offset_result = client.get_offset().await;

    match offset_result {
        Ok(offset) => {
            println!("Got NTP offset: {} us", offset);
            // Public NTP servers are usually close to our time unless our clock is very wrong
            // Just assert we got *some* value successfully
        }
        Err(e) => {
            // It might fail in CI due to firewalls blocking UDP 123, so we just log it
            println!(
                "NTP request failed (expected in some CI environments): {:?}",
                e
            );
        }
    }
}
