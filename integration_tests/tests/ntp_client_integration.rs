use std::time::Duration;

use airplay2::protocol::rtp::ntp_client::NtpClient;
use tokio;

#[tokio::test]
async fn test_ntp_client_against_public_server() {
    // Use pool.ntp.org as it might be more reliable in some environments than time.google.com,
    // and increase the timeout to 15s to prevent flaky CI failures.
    let servers = ["pool.ntp.org:123", "time.google.com:123", "time.apple.com:123"];

    let mut last_error = None;
    for server in servers {
        let client = NtpClient::new(server.to_string(), Duration::from_secs(10));
        match client.get_offset().await {
            Ok(offset) => {
                println!("Got NTP offset from {}: {} us", server, offset);
                return; // Success, we are done!
            }
            Err(e) => {
                println!("Failed to get offset from {}: {:?}", server, e);
                last_error = Some(e);
            }
        }
    }

    panic!(
        "NTP client failed to get offset from all tested public servers. Last error: {:?}",
        last_error
    );
}
