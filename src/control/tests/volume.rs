use crate::control::volume::Volume;

#[test]
fn test_volume_percent() {
    let vol = Volume::from_percent(50);
    assert_eq!(vol.as_percent(), 50);

    let vol = Volume::from_percent(100);
    assert!((vol.as_f32() - 1.0).abs() < f32::EPSILON);

    let vol = Volume::from_percent(0);
    assert!((vol.as_f32() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_volume_db() {
    let vol = Volume::MAX;
    assert!((vol.to_db() - 0.0).abs() < f32::EPSILON);

    let vol = Volume::MIN;
    assert!((vol.to_db() - -144.0).abs() < f32::EPSILON);

    // Test roundtrip
    let vol = Volume::new(0.5);
    let db = vol.to_db();
    let recovered = Volume::from_db(db);
    assert!((vol.as_f32() - recovered.as_f32()).abs() < 0.001);
}

#[test]
fn test_volume_clamping() {
    let vol = Volume::new(1.5);
    assert!((vol.as_f32() - 1.0).abs() < f32::EPSILON);

    let vol = Volume::new(-0.5);
    assert!((vol.as_f32() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_is_silent() {
    assert!(Volume::MIN.is_silent());
    assert!(Volume::new(0.0005).is_silent());
    assert!(!Volume::new(0.01).is_silent());
}

#[tokio::test]
async fn test_volume_controller_not_connected() {
    use std::sync::Arc;

    use crate::connection::ConnectionManager;
    use crate::control::volume::VolumeController;
    use crate::types::AirPlayConfig;

    let config = AirPlayConfig::default();
    let manager = Arc::new(ConnectionManager::new(config));
    let controller = VolumeController::new(manager);

    assert!(
        controller.set(Volume::new(0.5)).await.is_err(),
        "set volume should fail"
    );
    assert!(controller.mute().await.is_err(), "mute should fail");
    // `unmute` checks if muted, which is false initially, and returns `Ok(())` doing nothing.
    assert!(
        controller.unmute().await.is_ok(),
        "unmute should do nothing if not muted"
    );
    assert!(
        controller.toggle_mute().await.is_err(),
        "toggle_mute should fail"
    );
    assert!(controller.step_up().await.is_err(), "step_up should fail");
    assert!(
        controller.step_down().await.is_err(),
        "step_down should fail"
    );
}

#[tokio::test]
async fn test_group_volume_controller() {
    use std::sync::Arc;

    use crate::connection::ConnectionManager;
    use crate::control::volume::{GroupVolumeController, VolumeController};
    use crate::types::AirPlayConfig;

    let config = AirPlayConfig::default();
    let manager = Arc::new(ConnectionManager::new(config));
    let controller1 = Arc::new(VolumeController::new(manager.clone()));
    let controller2 = Arc::new(VolumeController::new(manager));

    let mut group_controller = GroupVolumeController::new();
    group_controller.add_device("d1".to_string(), controller1.clone());
    group_controller.add_device("d2".to_string(), controller2.clone());

    // Fails because connection is not established, but ensures mutability/struct access
    assert!(
        group_controller
            .set_master_volume(Volume::from_percent(50))
            .await
            .is_err()
    );
    assert!(group_controller.mute_all().await.is_err());
    // `unmute_all` iterates over devices and calls `unmute`, which returns `Ok(())` if they are not
    // muted.
    assert!(group_controller.unmute_all().await.is_ok());

    group_controller.remove_device("d1");
    // After removal, only 1 device
    assert!(
        group_controller
            .set_device_volume("d2", Volume::from_percent(80))
            .await
            .is_err()
    );
}
