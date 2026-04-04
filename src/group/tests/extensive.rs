use super::*;

#[tokio::test]
async fn test_create_multiple_groups() {
    let manager = GroupManager::new();

    let id1 = manager.create_group("Group 1").await;
    let id2 = manager.create_group("Group 2").await;

    let groups = manager.all_groups().await;
    assert_eq!(groups.len(), 2);

    let g1 = manager.get_group(&id1).await.unwrap();
    assert_eq!(g1.name, "Group 1");

    let g2 = manager.get_group(&id2).await.unwrap();
    assert_eq!(g2.name, "Group 2");
}

#[tokio::test]
async fn test_add_device_to_invalid_group() {
    let manager = GroupManager::new();
    let dev = test_device("d1");
    let id = GroupId::from_string("non-existent-group");

    let res = manager.add_device_to_group(&id, dev).await;
    assert!(
        res.is_err(),
        "Should return an error when group doesn't exist"
    );
}

#[tokio::test]
async fn test_find_device_group() {
    let manager = GroupManager::new();
    let dev = test_device("d1");

    let id1 = manager.create_group("Group 1").await;
    manager.add_device_to_group(&id1, dev).await.unwrap();

    let found_id = manager.find_device_group("d1").await;
    assert_eq!(found_id, Some(id1));

    let not_found_id = manager.find_device_group("d2").await;
    assert_eq!(not_found_id, None);
}

#[tokio::test]
async fn test_remove_last_member_deletes_group() {
    let manager = GroupManager::new();
    let dev = test_device("d1");

    let id1 = manager.create_group("Group 1").await;
    manager.add_device_to_group(&id1, dev).await.unwrap();

    let groups = manager.all_groups().await;
    assert_eq!(groups.len(), 1);

    manager.remove_device_from_group("d1").await.unwrap();

    let groups_after = manager.all_groups().await;
    assert_eq!(groups_after.len(), 0);
}

#[tokio::test]
async fn test_set_member_volume_invalid_device() {
    let manager = GroupManager::new();
    let dev = test_device("d1");

    let id1 = manager.create_group("Group 1").await;
    manager.add_device_to_group(&id1, dev).await.unwrap();

    // d2 is not in the group, setting its volume should not affect anything
    // It currently succeeds because it just looks up the group and then the member
    // and if the member isn't found it does nothing.
    let res = manager
        .set_member_volume(&id1, "d2", Volume::from_percent(50))
        .await;
    assert!(res.is_ok());

    let group = manager.get_group(&id1).await.unwrap();
    // Verify d1 effective volume is 75% (default group volume is 75% * member max 100%)
    assert_eq!(group.effective_volume("d1").as_percent(), 75);
}
