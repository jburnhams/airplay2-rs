use crate::control::queue::PlaybackQueue;
use crate::types::TrackInfo;

fn test_track(name: &str) -> TrackInfo {
    TrackInfo::new("http://example.com", name, "Artist")
}

#[test]
fn test_add_and_get() {
    let mut queue = PlaybackQueue::new();

    let id1 = queue.add(test_track("Track 1"));
    let _id2 = queue.add(test_track("Track 2"));

    assert_eq!(queue.len(), 2);
    assert_eq!(queue.get_by_id(id1).unwrap().track.title, "Track 1");
}

#[test]
fn test_navigation() {
    let mut queue = PlaybackQueue::new();

    queue.add(test_track("Track 1"));
    queue.add(test_track("Track 2"));
    queue.add(test_track("Track 3"));

    queue.set_current(0);
    assert_eq!(queue.current().unwrap().track.title, "Track 1");

    queue.advance();
    assert_eq!(queue.current().unwrap().track.title, "Track 2");

    queue.previous();
    assert_eq!(queue.current().unwrap().track.title, "Track 1");
}

#[test]
fn test_remove() {
    let mut queue = PlaybackQueue::new();

    let id1 = queue.add(test_track("Track 1"));
    queue.add(test_track("Track 2"));

    queue.set_current(1);
    queue.remove(id1);

    assert_eq!(queue.len(), 1);
    assert_eq!(queue.current_index(), Some(0));
}

#[test]
fn test_shuffle() {
    let mut queue = PlaybackQueue::new();

    for i in 0..10 {
        queue.add(test_track(&format!("Track {i}")));
    }

    queue.set_current(5);
    queue.shuffle();

    assert!(queue.is_shuffled());
    // Current track should still be current
    assert_eq!(queue.current().unwrap().track.title, "Track 5");
}

#[test]
fn test_insert_with_shuffle() {
    let mut queue = PlaybackQueue::new();
    queue.add(test_track("1"));
    queue.add(test_track("2"));

    queue.set_current(0);
    queue.shuffle(); // Order likely [0, 1] or [1, 0]

    // Insert "1.5" at index 1
    queue.insert(1, test_track("1.5"));
    // Items: "1", "1.5", "2"

    assert_eq!(queue.len(), 3);
    assert_eq!(queue.get(1).unwrap().track.title, "1.5");
    assert_eq!(queue.get(2).unwrap().track.title, "2");

    // Verify all 3 tracks are reachable in shuffle
    let mut count = 0;
    // Current is valid (shuffle_pos 0)
    if queue.current().is_some() {
        count += 1;
    }

    while queue.advance().is_some() {
        count += 1;
    }

    assert_eq!(count, 3, "Should play all 3 tracks in shuffle mode");
}

#[test]
fn test_move_track_with_shuffle() {
    let mut queue = PlaybackQueue::new();
    queue.add(test_track("A")); // 0
    queue.add(test_track("B")); // 1
    queue.add(test_track("C")); // 2

    queue.set_current(0);
    queue.shuffle();

    // Move "C" (2) to 0. New order: C, A, B.
    queue.move_track(2, 0);

    assert_eq!(queue.get(0).unwrap().track.title, "C");
    assert_eq!(queue.get(1).unwrap().track.title, "A");

    // Verify shuffle consistency (all items reachable)
    let mut titles = std::collections::HashSet::new();
    if let Some(c) = queue.current() {
        titles.insert(c.track.title.clone());
    }

    while let Some(item) = queue.advance() {
        titles.insert(item.track.title.clone());
    }

    assert_eq!(titles.len(), 3);
    assert!(titles.contains("A"));
    assert!(titles.contains("B"));
    assert!(titles.contains("C"));
}
