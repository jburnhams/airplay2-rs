use super::sequence_tracker::*;

#[test]
fn test_sequential_packets() {
    let mut tracker = SequenceTracker::new();

    assert!(tracker.record(100).is_none());
    assert!(tracker.record(101).is_none());
    assert!(tracker.record(102).is_none());

    assert_eq!(tracker.stats().packets_received, 3);
    assert_eq!(tracker.stats().total_lost, 0);
}

#[test]
fn test_gap_detection() {
    let mut tracker = SequenceTracker::new();

    tracker.record(100);
    let gap = tracker.record(105); // Skipped 101-104

    assert!(gap.is_some());
    let gap = gap.unwrap();
    assert_eq!(gap.start, 101);
    assert_eq!(gap.count, 4);
}

#[test]
fn test_wraparound() {
    let mut tracker = SequenceTracker::new();

    tracker.record(65534);
    tracker.record(65535);
    let gap = tracker.record(0); // Wrap to 0

    assert!(gap.is_none());
    assert_eq!(tracker.stats().total_lost, 0);
}

#[test]
fn test_loss_ratio() {
    let mut tracker = SequenceTracker::new();

    tracker.record(100);
    tracker.record(105); // Lost 4 packets (101, 102, 103, 104)
    // Received 2 (100, 105), Lost 4. Total = 6. Ratio = 4/6 = 0.666...

    assert!((tracker.loss_ratio() - 0.666).abs() < 0.01);
}
