use airplay2::protocol::rtp::ntp_client::NtpClient;
use std::time::Duration;
use tokio;

#[tokio::test]
#[ignore = "Hits public NTP server which might be flaky in CI and firewalls"]
async fn test_ntp_client_against_public_server() {
    let client = NtpClient::new("time.google.com:123".to_string(), Duration::from_secs(5));
    let offset_result = client.get_offset().await;

    assert!(
        offset_result.is_ok(),
        "NTP client failed to get offset from time.google.com: {:?}",
        offset_result.err()
    );

    let offset = offset_result.unwrap();
    println!("Got NTP offset: {} us", offset);
    // As long as the request returns a valid offset, we know packet encoding/decoding works correctly.
}
