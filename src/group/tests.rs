use super::manager::*;
use crate::control::volume::Volume;
use crate::types::AirPlayDevice;

fn test_device(id: &str) -> AirPlayDevice {
    AirPlayDevice {
        id: id.to_string(),
        name: format!("Device {}", id),
        model: None,
        address: "127.0.0.1".parse().unwrap(),
        port: 7000,
        capabilities: Default::default(),
        txt_records: Default::default(),
    }
}

#[test]
fn test_group_add_remove() {
    let mut group = DeviceGroup::new("Test Group");

    group.add_member(test_device("device1"));
    group.add_member(test_device("device2"));

    assert_eq!(group.member_count(), 2);
    assert!(group.members()[0].is_leader);
    assert!(!group.members()[1].is_leader);

    // Remove leader
    group.remove_member("device1");
    assert_eq!(group.member_count(), 1);
    assert!(group.members()[0].is_leader);
}

#[test]
fn test_effective_volume() {
    let mut group = DeviceGroup::new("Test");
    group.add_member(test_device("d1"));

    group.set_volume(Volume::from_percent(50));
    group.set_member_volume("d1", Volume::from_percent(80));

    let effective = group.effective_volume("d1");
    // 50% * 80% = 40%
    assert_eq!(effective.as_percent(), 40);
}

#[tokio::test]
async fn test_group_manager() {
    let manager = GroupManager::new();

    let group_id = manager.create_group("Living Room").await;
    manager
        .add_device_to_group(&group_id, test_device("speaker1"))
        .await
        .unwrap();
    manager
        .add_device_to_group(&group_id, test_device("speaker2"))
        .await
        .unwrap();

    let group = manager.get_group(&group_id).await.unwrap();
    assert_eq!(group.member_count(), 2);
}

#[tokio::test]
async fn test_create_group_with_devices() {
    let manager = GroupManager::new();
    let devices = vec![test_device("d1"), test_device("d2")];

    let group_id = manager
        .create_group_with_devices("Group 1", devices)
        .await
        .unwrap();

    let group = manager.get_group(&group_id).await.unwrap();
    assert_eq!(group.member_count(), 2);
}

#[tokio::test]
async fn test_create_group_with_devices_fail_already_grouped() {
    let manager = GroupManager::new();
    let d1 = test_device("d1");

    // First group
    let _g1 = manager
        .create_group_with_devices("Group 1", vec![d1.clone()])
        .await
        .unwrap();

    // Second group with same device
    let result = manager.create_group_with_devices("Group 2", vec![d1]).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_add_device_fail_already_grouped() {
    let manager = GroupManager::new();
    let d1 = test_device("d1");

    let g1 = manager.create_group("Group 1").await;
    manager.add_device_to_group(&g1, d1.clone()).await.unwrap();

    let g2 = manager.create_group("Group 2").await;
    let result = manager.add_device_to_group(&g2, d1).await;

    assert!(result.is_err());
}
