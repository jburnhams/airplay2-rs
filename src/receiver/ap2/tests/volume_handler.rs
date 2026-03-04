#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "Exact float comparison is safe and intended for clamped float outputs in tests"
)]
mod tests {
    use crate::receiver::ap2::volume_handler::VolumeController;

    #[test]
    fn test_volume_clamping() {
        let vc = VolumeController::new();

        vc.set_volume_db(-200.0);
        assert_eq!(vc.volume_db(), -144.0);

        vc.set_volume_db(10.0);
        assert_eq!(vc.volume_db(), 0.0);
    }

    #[test]
    fn test_volume_linear() {
        let vc = VolumeController::new();

        vc.set_volume_db(0.0);
        assert!((vc.volume_linear() - 1.0).abs() < f32::EPSILON);

        vc.set_volume_db(-20.0);
        assert!((vc.volume_linear() - 0.1).abs() < f32::EPSILON);

        vc.set_volume_db(-144.0);
        assert_eq!(vc.volume_linear(), 0.0);
    }

    #[test]
    fn test_handle_set_volume() {
        let vc = VolumeController::new();
        let body = b"volume: -15.5\r\n";

        let vol = vc.handle_set_volume(body).unwrap();
        assert_eq!(vol, -15.5);
        assert_eq!(vc.volume_db(), -15.5);
    }

    #[test]
    fn test_volume_muted() {
        let vc = VolumeController::new();
        assert!(!vc.is_muted()); // default is false

        vc.set_muted(true);
        assert!(vc.is_muted());

        vc.set_muted(false);
        assert!(!vc.is_muted());
    }

    #[test]
    fn test_handle_set_volume_errors() {
        let vc = VolumeController::new();

        // Invalid format
        let body = b"invalid format";
        assert!(vc.handle_set_volume(body).is_err());

        // Missing volume
        let body = b"key: value\r\n";
        assert!(vc.handle_set_volume(body).is_err());

        // Invalid volume value
        let body = b"volume: not_a_number\r\n";
        assert!(vc.handle_set_volume(body).is_err());
    }
}
