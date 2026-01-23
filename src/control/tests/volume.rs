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
