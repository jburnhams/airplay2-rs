use crate::protocol::rtsp::{Method, RtspRequest};
use crate::receiver::set_parameter_handler::{ParameterUpdate, process_set_parameter};

#[test]
fn test_process_volume() {
    let mut request = RtspRequest::new(Method::SetParameter, "rtsp://localhost");
    request.headers.insert("Content-Type", "text/parameters");
    request.body = b"volume: -20.0\r\n".to_vec();

    let updates = process_set_parameter(&request);
    assert_eq!(updates.len(), 1);

    match &updates[0] {
        ParameterUpdate::Volume(vol) => {
            assert!((vol.db - -20.0).abs() < 0.01);
        }
        _ => panic!("Expected Volume update"),
    }
}

#[test]
fn test_process_invalid_metadata() {
    let mut request = RtspRequest::new(Method::SetParameter, "rtsp://localhost");
    request.headers.insert("Content-Type", "application/x-dmap-tagged");
    request.body = b"invalid_dmap_data".to_vec();

    let updates = process_set_parameter(&request);
    assert!(updates.is_empty());
}
