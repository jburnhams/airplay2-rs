use crate::client::session::{AirPlaySession, RaopSessionImpl};

#[tokio::test]
async fn test_raop_set_volume() {
    let mut session = RaopSessionImpl::new("127.0.0.1", 5000);

    // Initial volume should be 1.0 (max)
    assert!((session.get_volume().await.unwrap() - 1.0).abs() < f32::EPSILON);

    // Set volume to 0.5
    session.set_volume(0.5).await.unwrap();
    assert!((session.get_volume().await.unwrap() - 0.5).abs() < f32::EPSILON);

    // Set volume to 0.0 (mute)
    session.set_volume(0.0).await.unwrap();
    assert!((session.get_volume().await.unwrap() - 0.0).abs() < f32::EPSILON);
}
