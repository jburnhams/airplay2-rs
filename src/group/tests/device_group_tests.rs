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
        last_seen: None,
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

#[test]
fn test_effective_volume_rounding() {
    let mut group = DeviceGroup::new("Round Test");
    group.add_member(test_device("d1"));

    // Set group to 50%, member to 15%
    group.set_volume(Volume::from_percent(50));
    group.set_member_volume("d1", Volume::from_percent(15));

    // 0.5 * 0.15 = 0.075 -> 7.5%, which should round to 8%
    let effective = group.effective_volume("d1");
    assert_eq!(effective.as_percent(), 8);
}

#[test]
fn test_device_group_with_leader() {
    let leader = test_device("leader");
    let group = DeviceGroup::with_leader("Leader Group", leader);

    assert_eq!(group.name, "Leader Group");
    assert_eq!(group.member_count(), 1);

    let member = group.member("leader").unwrap();
    assert!(member.is_leader);
    assert_eq!(member.volume, Volume::MAX); // Default individual volume

    // Group volume should be default
    assert_eq!(group.volume(), Volume::DEFAULT);
}
