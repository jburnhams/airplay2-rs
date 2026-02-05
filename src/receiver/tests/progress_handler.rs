use crate::receiver::progress_handler::{parse_progress, PlaybackProgress};

#[test]
fn test_parse_progress() {
    let body = "progress: 0.0/30.5/180.0\r\n";
    let progress = parse_progress(body).unwrap();

    assert!((progress.start - 0.0).abs() < 0.01);
    assert!((progress.current - 30.5).abs() < 0.01);
    assert!((progress.end - 180.0).abs() < 0.01);
}

#[test]
fn test_progress_percentage() {
    let progress = PlaybackProgress {
        start: 0.0,
        current: 60.0,
        end: 120.0,
    };

    assert!((progress.percentage() - 0.5).abs() < 0.01);
}
