use crate::receiver::volume_handler::{db_to_linear, linear_to_db, parse_volume_parameter};

#[test]
fn test_parse_volume() {
    let body = "volume: -15.000000\r\n";
    let update = parse_volume_parameter(body).unwrap();

    assert!((update.db - -15.0).abs() < 0.01);
    assert!(!update.muted);
}

#[test]
fn test_parse_muted() {
    let body = "volume: -144.000000\r\n";
    let update = parse_volume_parameter(body).unwrap();

    assert!(update.muted);
    assert!(update.linear < 0.001);
}

#[test]
fn test_db_to_linear() {
    assert!((db_to_linear(0.0) - 1.0).abs() < 0.001);
    assert!((db_to_linear(-20.0) - 0.1).abs() < 0.01);
    assert!(db_to_linear(-144.0) < 0.001);
}

#[test]
fn test_linear_to_db() {
    assert!((linear_to_db(1.0) - 0.0).abs() < 0.01);
    assert!((linear_to_db(0.1) - -20.0).abs() < 0.1);
}
