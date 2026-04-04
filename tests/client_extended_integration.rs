use airplay2::{AirPlayError, UnifiedAirPlayClient};

mod common;

#[tokio::test]
async fn test_unified_client_creation() {
    common::init_logging();

    let client = UnifiedAirPlayClient::new();
    assert!(!client.is_connected());
    assert!(client.protocol().is_none());
    assert!(client.session().is_none());
}

#[tokio::test]
async fn test_unified_client_disconnected_errors() {
    common::init_logging();

    let mut client = UnifiedAirPlayClient::new();

    let res = client.play().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = client.pause().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = client.stop().await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = client.set_volume(0.5).await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));

    let res = client.stream_audio(&[0, 0, 0, 0]).await;
    assert!(matches!(res, Err(AirPlayError::Disconnected { .. })));
}

#[tokio::test]
async fn test_client_forget_device() {
    common::init_logging();

    let client = airplay2::AirPlayClient::default_client();
    let result = client.forget_device("SomeDeviceID").await;

    // We expect Ok(()) since forgetting a non-existent device or without storage
    // simply returns Ok(()) or the connection manager handles it smoothly.
    assert!(result.is_ok(), "Expected forget_device to return Ok");
}

#[tokio::test]
async fn test_client_ptp_clock_disconnected() {
    common::init_logging();

    let client = airplay2::AirPlayClient::default_client();

    // When not connected, PTP should not be active
    assert!(!client.is_ptp_active().await);

    // The shared PTP clock should be None
    assert!(client.ptp_clock().await.is_none());
}
