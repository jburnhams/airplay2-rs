use crate::group::manager::*;
use crate::group::tests::test_device;

#[tokio::test]
async fn test_leader_reassigned_on_remove_correctly() {
    let manager = GroupManager::new();
    let d1 = test_device("d1");
    let d2 = test_device("d2");
    let d3 = test_device("d3");

    let group_id = manager
        .create_group_with_devices("Leader Test", vec![d1.clone(), d2.clone(), d3.clone()])
        .await
        .unwrap();

    let group = manager.get_group(&group_id).await.unwrap();
    assert!(group.member("d1").unwrap().is_leader);

    manager.remove_device_from_group("d1").await.unwrap();

    let group_after_remove = manager.get_group(&group_id).await.unwrap();
    assert_eq!(group_after_remove.member_count(), 2);
    // d2 should have been promoted to leader
    assert!(group_after_remove.member("d2").unwrap().is_leader);
    assert!(!group_after_remove.member("d3").unwrap().is_leader);
}

#[test]
fn test_member_and_all_connected() {
    let mut group = DeviceGroup::new("Test Group");

    let d1 = test_device("d1");
    let d2 = test_device("d2");

    group.add_member(d1.clone());
    group.add_member(d2.clone());

    assert_eq!(group.connected_count(), 0);
    assert!(!group.all_connected());

    if let Some(m) = group.member_mut("d1") {
        m.connected = true;
    }

    assert_eq!(group.connected_count(), 1);
    assert!(!group.all_connected());

    if let Some(m) = group.member_mut("d2") {
        m.connected = true;
    }

    assert_eq!(group.connected_count(), 2);
    assert!(group.all_connected());
}
