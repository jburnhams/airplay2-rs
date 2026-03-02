use std::collections::HashMap;

use crate::control::volume::Volume;
use crate::group::manager::*;
use crate::types::{AirPlayDevice, DeviceCapabilities};

fn test_device(id: &str) -> AirPlayDevice {
    AirPlayDevice {
        id: id.to_string(),
        name: format!("Device {id}"),
        model: None,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        port: 7000,
        capabilities: DeviceCapabilities::default(),
        raop_port: None,
        raop_capabilities: None,
        txt_records: HashMap::default(),
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

#[tokio::test]
async fn test_delete_group() {
    let manager = GroupManager::new();
    let d1 = test_device("d1");
    let d2 = test_device("d2");

    let group_id = manager
        .create_group_with_devices("Test Delete", vec![d1.clone(), d2.clone()])
        .await
        .unwrap();

    // Verify devices are in the group
    assert_eq!(
        manager.find_device_group("d1").await,
        Some(group_id.clone())
    );
    assert_eq!(
        manager.find_device_group("d2").await,
        Some(group_id.clone())
    );

    // Delete group
    let deleted_group = manager.delete_group(&group_id).await;
    assert!(deleted_group.is_some());
    assert_eq!(deleted_group.unwrap().name, "Test Delete");

    // Verify group is gone
    assert!(manager.get_group(&group_id).await.is_none());

    // Verify devices are no longer mapped to the group
    assert!(manager.find_device_group("d1").await.is_none());
    assert!(manager.find_device_group("d2").await.is_none());
}

#[tokio::test]
async fn test_remove_device_from_group() {
    let manager = GroupManager::new();
    let d1 = test_device("d1");
    let d2 = test_device("d2");

    let group_id = manager
        .create_group_with_devices("Test Remove", vec![d1.clone(), d2.clone()])
        .await
        .unwrap();

    // Remove d1
    let result = manager.remove_device_from_group("d1").await;
    assert!(result.is_ok());

    // Verify d1 is no longer in group mapping
    assert!(manager.find_device_group("d1").await.is_none());

    // Verify group member count is 1
    let group = manager.get_group(&group_id).await.unwrap();
    assert_eq!(group.member_count(), 1);
    assert!(group.member("d1").is_none());
    assert!(group.member("d2").is_some());

    // Verify d2 was promoted to leader if it wasn't already
    assert!(group.leader().is_some());
    assert_eq!(group.leader().unwrap().device.id, "d2");

    // Remove d2 (last member)
    let result = manager.remove_device_from_group("d2").await;
    assert!(result.is_ok());

    // Verify group is automatically deleted when empty
    assert!(manager.get_group(&group_id).await.is_none());
}

#[tokio::test]
async fn test_all_groups() {
    let manager = GroupManager::new();
    let d1 = test_device("d1");
    let d2 = test_device("d2");

    let _g1 = manager
        .create_group_with_devices("G1", vec![d1])
        .await
        .unwrap();
    let _g2 = manager
        .create_group_with_devices("G2", vec![d2])
        .await
        .unwrap();

    let groups = manager.all_groups().await;
    assert_eq!(groups.len(), 2);
    let names: Vec<String> = groups.into_iter().map(|g| g.name).collect();
    assert!(names.contains(&"G1".to_string()));
    assert!(names.contains(&"G2".to_string()));
}

#[tokio::test]
async fn test_set_volumes() {
    let manager = GroupManager::new();
    let d1 = test_device("d1");
    let d2 = test_device("d2");

    let group_id = manager
        .create_group_with_devices("Volume Test", vec![d1.clone(), d2.clone()])
        .await
        .unwrap();

    // Set group volume
    let result = manager
        .set_group_volume(&group_id, Volume::from_percent(50))
        .await;
    assert!(result.is_ok());

    // Set member volume
    let result = manager
        .set_member_volume(&group_id, "d1", Volume::from_percent(80))
        .await;
    assert!(result.is_ok());

    // Verify group volume
    let group = manager.get_group(&group_id).await.unwrap();
    assert_eq!(group.volume().as_percent(), 50);

    // Verify effective volumes
    // d1: 50% * 80% = 40%
    assert_eq!(group.effective_volume("d1").as_percent(), 40);
    // d2: 50% * 100% (default max) = 50%
    assert_eq!(group.effective_volume("d2").as_percent(), 50);
}

#[tokio::test]
async fn test_invalid_device_not_found() {
    let manager = GroupManager::new();
    let group_id = GroupId::from_string("nonexistent");

    let result = manager.set_group_volume(&group_id, Volume::MAX).await;
    assert!(result.is_err());

    let result = manager
        .set_member_volume(&group_id, "d1", Volume::MAX)
        .await;
    assert!(result.is_err());
}
