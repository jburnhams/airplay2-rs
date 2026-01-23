use super::container::*;
use super::events::*;
use crate::types::PlaybackState;

#[tokio::test]
async fn test_state_update() {
    let container = StateContainer::new();

    container.set_volume(0.5).await;
    let state = container.get().await;

    assert!((state.volume - 0.5).abs() < f32::EPSILON);
}

#[tokio::test]
async fn test_state_subscription() {
    let container = StateContainer::new();
    let mut rx = container.subscribe();

    // Initial state
    assert!((rx.borrow().volume - 1.0).abs() < f32::EPSILON);

    container.set_volume(0.75).await;

    // Receiver should have the updated state
    rx.changed().await.unwrap();
    assert!((rx.borrow().volume - 0.75).abs() < f32::EPSILON);
}

#[tokio::test]
async fn test_event_bus() {
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    bus.emit(ClientEvent::VolumeChanged { volume: 0.5 });

    let event = rx.recv().await.unwrap();
    if let ClientEvent::VolumeChanged { volume } = event {
        assert!((volume - 0.5).abs() < f32::EPSILON);
    } else {
        panic!("Wrong event type");
    }
}

#[tokio::test]
async fn test_event_filter() {
    let bus = EventBus::new();
    let mut filter = EventFilter::playback_events(&bus);

    // Emit non-playback event
    bus.emit(ClientEvent::VolumeChanged { volume: 0.5 });
    // Emit playback event
    bus.emit(ClientEvent::TrackChanged { track: None });

    // Filter should only receive playback event
    let event = filter.recv().await.unwrap();
    assert!(matches!(event, ClientEvent::TrackChanged { .. }));
}

#[tokio::test]
async fn test_playback_state_event() {
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    let state = PlaybackState::default();
    bus.emit(ClientEvent::PlaybackStateChanged {
        old: Box::new(state.clone()),
        new: Box::new(state),
    });

    let event = rx.recv().await.unwrap();
    if let ClientEvent::PlaybackStateChanged { old, new } = event {
        assert!((old.volume - new.volume).abs() < f32::EPSILON);
    } else {
        panic!("Wrong event type");
    }
}
