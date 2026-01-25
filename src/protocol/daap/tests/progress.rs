use crate::protocol::daap::DmapProgress;

#[test]
fn test_progress_encode() {
    let progress = DmapProgress::new(0, 44_100, 441_000);
    let encoded = progress.encode();

    assert_eq!(encoded, "progress: 0/44100/441000\r\n");
}

#[test]
fn test_progress_parse() {
    let text = "progress: 1000/2000/3000\r\n";
    let progress = DmapProgress::parse(text).unwrap();

    assert_eq!(progress.start, 1000);
    assert_eq!(progress.current, 2000);
    assert_eq!(progress.end, 3000);
}

#[test]
fn test_progress_percentage() {
    let progress = DmapProgress::new(0, 50, 100);
    assert!((progress.percentage() - 0.5).abs() < f64::EPSILON);

    let progress = DmapProgress::new(0, 0, 100);
    assert!((progress.percentage() - 0.0).abs() < f64::EPSILON);

    let progress = DmapProgress::new(0, 100, 100);
    assert!((progress.percentage() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_progress_from_samples() {
    // 10 seconds at 44.1kHz
    let progress = DmapProgress::from_samples(1000, 441_000, 4_410_000);

    assert_eq!(progress.start, 1000);
    assert_eq!(progress.current, 442_000);
    assert_eq!(progress.end, 4_411_000);
}
