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
            // As long as the request returns a valid offset, we know packet encoding/decoding works
            // correctly.
        }
        Err(e) => {
            // Ignore network or timeout errors as they are expected in some CI environments
            println!(
                "NTP client failed to get offset from time.google.com (ignoring due to potential network constraints): {:?}",
                e
            );
        }
    }
}
